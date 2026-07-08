
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    metadata::{
        create_metadata_accounts_v3,
        mpl_token_metadata::types::{
            Collection, CollectionDetails, Creator, DataV2,
        },
        set_and_verify_sized_collection_item,
        CreateMetadataAccountsV3,
        Metadata,
        SetAndVerifySizedCollectionItem,
    },
    token::{self, Mint, Token, TokenAccount},
};
use mpl_bubblegum::{
    instructions::MintToCollectionV1CpiBuilder,
    types::{
        Collection as BubblegumCollection,
        Creator as BubblegumCreator,
        MetadataArgs,
        TokenProgramVersion,
        TokenStandard,  
    },
};
declare_id!("3TphjLz52Xv9a2sW9C56dA31ouXZRkwdaPhhjJZWYjvK");

// Well-known, permanent program IDs. Hardcoded to avoid depending on the
// Solana 1.16-era spl-account-compression / spl-noop crates, which conflict
// with the modern (zeroize >= 1.5) toolchain.
pub const SPL_NOOP_ID: Pubkey = pubkey!("noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV");
pub const SPL_ACCOUNT_COMPRESSION_ID: Pubkey =
    pubkey!("cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK");

#[program]
pub mod solana_nft_marketplace {
    use super::*;

    /// Initializes collection config PDA.
    pub fn initialize_collection(
        ctx: Context<InitializeCollection>,
        royalty_bps: u16,
    ) -> Result<()> {
        require!(royalty_bps <= 10_000, MarketplaceError::InvalidRoyalty);

        let config = &mut ctx.accounts.collection_config;
        config.authority    = ctx.accounts.authority.key();
        config.royalty_bps  = royalty_bps;
        config.tree_address = Pubkey::default();
        config.total_minted = 0;
        config.is_active    = true;
        config.bump         = ctx.bumps.collection_config;

        msg!(
            "Collection initialized. Authority: {}. Royalty: {}bps.",
            config.authority,
            config.royalty_bps
        );

        Ok(())
    }

    /// Creates the collection NFT — the on-chain identity of the collection.
    /// Must be called once before any member NFTs can be minted.
    pub fn create_collection_nft(
        ctx: Context<CreateCollectionNft>,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        let config = &mut ctx.accounts.collection_config;

        // Ensure collection NFT hasn't been created yet
        require!(
            config.collection_mint == Pubkey::default(),
            MarketplaceError::CollectionAlreadyCreated
        );

        require!(!uri.is_empty(), MarketplaceError::InvalidUri);

        // Step 1: Mint 1 token to authority's ATA
        token::mint_to(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info().key(),
                token::MintTo {
                    mint:      ctx.accounts.collection_mint.to_account_info(),
                    to:        ctx.accounts.authority_token_account.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            1,
        )?;

        // Step 2: Create metadata with CollectionDetails
        // CollectionDetails marks this as a sized collection NFT
        create_metadata_accounts_v3(
            CpiContext::new(
                ctx.accounts.token_metadata_program.to_account_info().key(),
                CreateMetadataAccountsV3 {
                    metadata:         ctx.accounts.collection_metadata.to_account_info(),
                    mint:             ctx.accounts.collection_mint.to_account_info(),
                    mint_authority:   ctx.accounts.authority.to_account_info(),
                    payer:            ctx.accounts.authority.to_account_info(),
                    update_authority: ctx.accounts.authority.to_account_info(),
                    system_program:   ctx.accounts.system_program.to_account_info(),
                    rent:             ctx.accounts.rent.to_account_info(),
                },
            ),
            DataV2 {
                name:                    name.clone(),
                symbol:                  symbol.clone(),
                uri:                     uri.clone(),
                seller_fee_basis_points: config.royalty_bps,
                creators: Some(vec![Creator {
                    address:  ctx.accounts.authority.key(),
                    verified: true,
                    share:    100,
                }]),
                collection: None, // collection NFT has no parent
                uses:       None,
            },
            true,  // is_mutable
            true,  // update_authority_is_signer
            // CollectionDetails::V1 marks this as a sized collection
            // size starts at 0 and increments as members are verified
            Some(CollectionDetails::V1 { size: 0 }),
        )?;

        // Step 3: Revoke mint authority
        token::set_authority(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info().key(),
                token::SetAuthority {
                    account_or_mint:   ctx.accounts.collection_mint.to_account_info(),
                    current_authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            token::spl_token::instruction::AuthorityType::MintTokens,
            None,
        )?;

        // Step 4: Store collection mint in config
        config.collection_mint = ctx.accounts.collection_mint.key();

        msg!(
            "Collection NFT created: {}. Name: {}.",
            ctx.accounts.collection_mint.key(),
            name
        );

        Ok(())
    }

    /// Creates the Merkle tree that will hold all compressed NFTs.
    /// Must be called before any cNFTs can be minted.
    pub fn create_tree(
        ctx: Context<CreateTree>,
        max_depth: u32,
        max_buffer_size: u32
    )-> Result<()> {
        let config = &mut ctx.accounts.collection_config;

        // before creating make sure it is not exist already
        require!(config.tree_address == Pubkey::default(),
        MarketplaceError::TreeAlreadyCreated
        );

        // CPI into SPL Account Compression via Bubblegum
        // Bubblegum handles the tree_config account creation
        // SPL Compression handles the actual Merkle tree account
        mpl_bubblegum::instructions::CreateTreeConfigCpiBuilder::new(
            &ctx.accounts.bubblegum_program.to_account_info()

        ).tree_config(&ctx.accounts.tree_config.to_account_info())
        .merkle_tree(&ctx.accounts.merkle_tree.to_account_info())
        .payer(&ctx.accounts.authority.to_account_info())
        .tree_creator(&ctx.accounts.authority.to_account_info())
        .log_wrapper(&ctx.accounts.log_wrapper.to_account_info())
        .compression_program(&ctx.accounts.compression_program.to_account_info())
        .system_program(&ctx.accounts.system_program.to_account_info())
        .max_depth(max_depth)
        .max_buffer_size(max_buffer_size)
        .invoke()?;

        config.tree_address = ctx.accounts.merkle_tree.key();


        msg!(
            "Merkle tree created: {}. Depth: {}. Buffer: {}.",
            ctx.accounts.merkle_tree.key(),
            max_depth,
            max_buffer_size
        );

        Ok(())
    }

    //Mint Compressed NFT intpo the collections Merkle tree
    pub fn mint_compressed_nft(
        ctx: Context<MintCompressedNft>,
        name: String,
        symbol: String,
        uri: String,
    )-> Result<()>{

        let config = &mut ctx.accounts.collection_config;
        require!(config.is_active, MarketplaceError::CollectionInactive);
        require!(!uri.is_empty(), MarketplaceError::InvalidUri);
        require!(
            config.tree_address != Pubkey::default(),
            MarketplaceError::TreeNotCreated
        );
        require!(
            config.collection_mint != Pubkey::default(),
            MarketplaceError::CollectionNotCreated
        );

        //Build the metadata for this compresses NFT
        let metadata_args = MetadataArgs {
            name: name.clone(),
            symbol: symbol.clone(),
            uri: uri.clone(),
          seller_fee_basis_points: config.royalty_bps,
            primary_sale_happened:   false,
            is_mutable:              true,
            edition_nonce:           None,
            token_standard:          Some(TokenStandard::NonFungible),
            collection: Some(BubblegumCollection {
                key:      config.collection_mint,
                verified: false, // Bubblegum verifies this during the CPI
            }),
            uses:          None,
            token_program_version: TokenProgramVersion::Original,
            creators: vec![BubblegumCreator {
                address:  ctx.accounts.authority.key(),
                verified: true,
                share:    100,
            }],
        };

         // CPI into Bubblegum mint_to_collection_v1
        // This simultaneously:
        // 1. Adds a leaf to the Merkle tree
        // 2. Verifies the NFT belongs to the collection
        // 3. Emits a structured log event for indexers
        MintToCollectionV1CpiBuilder::new(
            &ctx.accounts.bubblegum_program.to_account_info(),
        )
        .tree_config(&ctx.accounts.tree_config.to_account_info())
        .leaf_owner(&ctx.accounts.leaf_owner.to_account_info())
        .leaf_delegate(&ctx.accounts.leaf_owner.to_account_info())
        .merkle_tree(&ctx.accounts.merkle_tree.to_account_info())
        .payer(&ctx.accounts.authority.to_account_info())
        .tree_creator_or_delegate(&ctx.accounts.authority.to_account_info())
        .collection_authority(&ctx.accounts.authority.to_account_info())
        // None: the collection authority is the direct update authority, so
        // there is no legacy Token Metadata authority-record PDA to pass.
        .collection_authority_record_pda(None)
        .collection_mint(&ctx.accounts.collection_mint.to_account_info())
        .collection_metadata(&ctx.accounts.collection_metadata.to_account_info())
        .collection_edition(
            &ctx.accounts.collection_master_edition.to_account_info()
        )
        .bubblegum_signer(&ctx.accounts.bubblegum_signer.to_account_info())
        .log_wrapper(&ctx.accounts.log_wrapper.to_account_info())
        .compression_program(&ctx.accounts.compression_program.to_account_info())
        .token_metadata_program(
            &ctx.accounts.token_metadata_program.to_account_info()
        )
        .system_program(&ctx.accounts.system_program.to_account_info())
        .metadata(metadata_args)
        .invoke()?;

        // Update collection state
        config.total_minted += 1;

        msg!(
            "Compressed NFT minted. Owner: {}. Total: {}.",
            ctx.accounts.leaf_owner.key(),
            config.total_minted
        );

        Ok(())
    }
    /// Mints a member NFT and verifies it belongs to the collection.
    pub fn mint_nft(
        ctx: Context<MintNft>,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        let config = &mut ctx.accounts.collection_config;

        require!(config.is_active, MarketplaceError::CollectionInactive);
        require!(!uri.is_empty(), MarketplaceError::InvalidUri);

        // Ensure collection NFT exists before minting members
        require!(
            config.collection_mint != Pubkey::default(),
            MarketplaceError::CollectionNotCreated
        );

        // Step 1: Mint 1 token
        token::mint_to(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info().key(),
                token::MintTo {
                    mint:      ctx.accounts.mint.to_account_info(),
                    to:        ctx.accounts.creator_token_account.to_account_info(),
                    authority: ctx.accounts.creator.to_account_info(),
                },
            ),
            1,
        )?;

        // Step 2: Create metadata with collection reference
        create_metadata_accounts_v3(
            CpiContext::new(
                ctx.accounts.token_metadata_program.to_account_info().key(),
                CreateMetadataAccountsV3 {
                    metadata:         ctx.accounts.metadata.to_account_info(),
                    mint:             ctx.accounts.mint.to_account_info(),
                    mint_authority:   ctx.accounts.creator.to_account_info(),
                    payer:            ctx.accounts.creator.to_account_info(),
                    update_authority: ctx.accounts.creator.to_account_info(),
                    system_program:   ctx.accounts.system_program.to_account_info(),
                    rent:             ctx.accounts.rent.to_account_info(),
                },
            ),
            DataV2 {
                name:                    name.clone(),
                symbol:                  symbol.clone(),
                uri:                     uri.clone(),
                seller_fee_basis_points: config.royalty_bps,
                creators: Some(vec![Creator {
                    address:  ctx.accounts.creator.key(),
                    verified: true,
                    share:    100,
                }]),
                // Reference the collection — verified=false until next step
                collection: Some(Collection {
                    key:      config.collection_mint,
                    verified: false,
                }),
                uses: None,
            },
            true,
            true,
            None,
        )?;

        // Step 3: Verify the NFT belongs to the collection
        // This sets verified=true and increments collection size
        set_and_verify_sized_collection_item(
            CpiContext::new(
                ctx.accounts.token_metadata_program.to_account_info().key(),
                SetAndVerifySizedCollectionItem {
                    metadata:               ctx.accounts.metadata.to_account_info(),
                    collection_authority:   ctx.accounts.creator.to_account_info(),
                    payer:                  ctx.accounts.creator.to_account_info(),
                    update_authority:       ctx.accounts.creator.to_account_info(),
                    collection_mint:        ctx.accounts.collection_mint.to_account_info(),
                    collection_metadata:    ctx.accounts.collection_metadata.to_account_info(),
                    collection_master_edition: ctx.accounts.collection_master_edition.to_account_info(),
                },
            ),
            None, // collection_authority_record — None means direct authority
        )?;

        // Step 4: Revoke mint authority
        token::set_authority(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info().key(),
                token::SetAuthority {
                    account_or_mint:   ctx.accounts.mint.to_account_info(),
                    current_authority: ctx.accounts.creator.to_account_info(),
                },
            ),
            token::spl_token::instruction::AuthorityType::MintTokens,
            None,
        )?;

        // Step 5: Update collection state
        config.total_minted += 1;

        msg!(
            "NFT minted and verified: {}. Total: {}.",
            ctx.accounts.mint.key(),
            config.total_minted
        );

        Ok(())
    }
}

// ────────────────────────────────────────────────
// Account Contexts
// ────────────────────────────────────────────────

#[derive(Accounts)]
pub struct InitializeCollection<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = CollectionConfig::SIZE,
        seeds = [b"config", authority.key().as_ref()],
        bump,
    )]
    pub collection_config: Account<'info, CollectionConfig>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateCollectionNft<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        mint::decimals = 0,
        mint::authority = authority,
        mint::freeze_authority = authority,
    )]
    pub collection_mint: Account<'info, Mint>,

    /// CHECK: Validated by Metaplex via CPI
    #[account(
        mut,
        seeds = [
            b"metadata",
            token_metadata_program.key().as_ref(),
            collection_mint.key().as_ref(),
        ],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub collection_metadata: UncheckedAccount<'info>,

    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = collection_mint,
        associated_token::authority = authority,
    )]
    pub authority_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"config", authority.key().as_ref()],
        bump = collection_config.bump,
        constraint = collection_config.authority == authority.key()
            @ MarketplaceError::UnauthorizedMinter,
    )]
    pub collection_config: Account<'info, CollectionConfig>,

    pub token_program:            Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_metadata_program:   Program<'info, Metadata>,
    pub system_program:           Program<'info, System>,
    pub rent:                     Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct CreateTree<'info> {
    /// Collection authority — pays for tree account
    #[account(mut)]
    pub authority: Signer<'info>,

    /// Bubblegum tree config PDA
    /// CHECK: Validated by Bubblegum via CPI
    #[account(
        mut,
        seeds = [merkle_tree.key().as_ref()],
        bump,
        seeds::program = bubblegum_program.key(),
    )]
    pub tree_config: UncheckedAccount<'info>,

    /// The actual Merkle tree account — must be a fresh keypair
    /// CHECK: Validated by SPL Account Compression via CPI
    #[account(mut)]
    pub merkle_tree: UncheckedAccount<'info>,

    /// Collection config — updated with tree address
    #[account(
        mut,
        seeds = [b"config", authority.key().as_ref()],
        bump = collection_config.bump,
        constraint = collection_config.authority == authority.key()
            @ MarketplaceError::UnauthorizedMinter,
    )]
    pub collection_config: Account<'info, CollectionConfig>,

    /// CHECK: Bubblegum program
    #[account(address = mpl_bubblegum::ID)]
    pub bubblegum_program: UncheckedAccount<'info>,

    /// CHECK: SPL Noop program for logging
    #[account(address = SPL_NOOP_ID)]
    pub log_wrapper: UncheckedAccount<'info>,

    /// CHECK: SPL Account Compression program
    #[account(address = SPL_ACCOUNT_COMPRESSION_ID)]
    pub compression_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintCompressedNft<'info> {
    /// Collection authority — signs the mint
    #[account(mut)]
    pub authority: Signer<'info>,

    /// Who receives the cNFT
    /// CHECK: Any valid pubkey can receive
    pub leaf_owner: UncheckedAccount<'info>,

    /// Bubblegum tree config
    /// CHECK: Validated by Bubblegum
    #[account(
        mut,
        seeds = [merkle_tree.key().as_ref()],
        bump,
        seeds::program = bubblegum_program.key(),
    )]
    pub tree_config: UncheckedAccount<'info>,

    /// The Merkle tree
    /// CHECK: Validated by SPL Compression
    #[account(
        mut,
        constraint = merkle_tree.key() == collection_config.tree_address
            @ MarketplaceError::InvalidTree,
    )]
    pub merkle_tree: UncheckedAccount<'info>,

    /// Collection mint
    /// CHECK: Validated by Bubblegum
    #[account(
        constraint = collection_mint.key() == collection_config.collection_mint
            @ MarketplaceError::InvalidCollectionMint,
    )]
    pub collection_mint: UncheckedAccount<'info>,

    /// Collection metadata PDA
    /// CHECK: Validated by Bubblegum
    #[account(
        mut,
        seeds = [
            b"metadata",
            token_metadata_program.key().as_ref(),
            collection_mint.key().as_ref(),
        ],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub collection_metadata: UncheckedAccount<'info>,

    /// Collection master edition
    /// CHECK: Validated by Bubblegum
    #[account(
        mut,
        seeds = [
            b"metadata",
            token_metadata_program.key().as_ref(),
            collection_mint.key().as_ref(),
            b"edition",
        ],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub collection_master_edition: UncheckedAccount<'info>,

    /// Bubblegum signer PDA
    /// CHECK: Validated by Bubblegum
    #[account(
        seeds = [b"collection_cpi"],
        bump,
        seeds::program = bubblegum_program.key(),
    )]
    pub bubblegum_signer: UncheckedAccount<'info>,

    /// Collection config
    #[account(
        mut,
        seeds = [b"config", authority.key().as_ref()],
        bump = collection_config.bump,
        constraint = collection_config.authority == authority.key()
            @ MarketplaceError::UnauthorizedMinter,
    )]
    pub collection_config: Account<'info, CollectionConfig>,

    /// CHECK: Bubblegum program
    #[account(address = mpl_bubblegum::ID)]
    pub bubblegum_program: UncheckedAccount<'info>,

    /// CHECK: SPL Noop for logging
    #[account(address = SPL_NOOP_ID)]
    pub log_wrapper: UncheckedAccount<'info>,

    /// CHECK: SPL Account Compression
    #[account(address = SPL_ACCOUNT_COMPRESSION_ID)]
    pub compression_program: UncheckedAccount<'info>,

    pub token_metadata_program: Program<'info, Metadata>,
    pub system_program:         Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintNft<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        init,
        payer = creator,
        mint::decimals = 0,
        mint::authority = creator,
        mint::freeze_authority = creator,
    )]
    pub mint: Account<'info, Mint>,

    /// CHECK: Validated by Metaplex via CPI
    #[account(
        mut,
        seeds = [
            b"metadata",
            token_metadata_program.key().as_ref(),
            mint.key().as_ref(),
        ],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub metadata: UncheckedAccount<'info>,

    #[account(
        init_if_needed,
        payer = creator,
        associated_token::mint = mint,
        associated_token::authority = creator,
    )]
    pub creator_token_account: Account<'info, TokenAccount>,

    /// CHECK: Validated by Metaplex via CPI
    #[account(
        mut,
        constraint = collection_mint.key() == collection_config.collection_mint
            @ MarketplaceError::InvalidCollectionMint,
    )]
    pub collection_mint: UncheckedAccount<'info>,

    /// CHECK: Validated by Metaplex via CPI
    #[account(
        mut,
        seeds = [
            b"metadata",
            token_metadata_program.key().as_ref(),
            collection_mint.key().as_ref(),
        ],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub collection_metadata: UncheckedAccount<'info>,

    /// CHECK: Validated by Metaplex via CPI
    #[account(
        mut,
        seeds = [
            b"metadata",
            token_metadata_program.key().as_ref(),
            collection_mint.key().as_ref(),
            b"edition",
        ],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub collection_master_edition: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [b"config", creator.key().as_ref()],
        bump = collection_config.bump,
        constraint = collection_config.authority == creator.key()
            @ MarketplaceError::UnauthorizedMinter,
    )]
    pub collection_config: Account<'info, CollectionConfig>,

    pub token_program:            Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_metadata_program:   Program<'info, Metadata>,
    pub system_program:           Program<'info, System>,
    pub rent:                     Sysvar<'info, Rent>,
}

// ────────────────────────────────────────────────
// Account Structs
// ────────────────────────────────────────────────

#[account]
pub struct CollectionConfig {
    pub authority:       Pubkey,
    pub royalty_bps:     u16,
    pub tree_address:    Pubkey,
    pub collection_mint: Pubkey,
    pub total_minted:    u64,
    pub is_active:       bool,
    pub bump:            u8,
}

impl CollectionConfig {
    pub const SIZE: usize = 8 + 32 + 2 + 32 + 32 + 8 + 1 + 1;
}

// ────────────────────────────────────────────────
// Errors
// ────────────────────────────────────────────────

#[error_code]
pub enum MarketplaceError {
    #[msg("Royalty basis points cannot exceed 10,000 (100%)")]
    InvalidRoyalty,

    #[msg("Collection is not active")]
    CollectionInactive,

    #[msg("Only the collection authority can mint")]
    UnauthorizedMinter,

    #[msg("Metadata URI cannot be empty")]
    InvalidUri,

    #[msg("Collection NFT already created")]
    CollectionAlreadyCreated,

    #[msg("Collection NFT must be created before minting members")]
    CollectionNotCreated,

    #[msg("Invalid collection mint address")]
    InvalidCollectionMint,

    #[msg("Merkle tree already created")]
    TreeAlreadyCreated,

    #[msg("Merkle tree must be created before minting compressed NFTs")]
    TreeNotCreated,

    #[msg("Invalid Merkle tree address")]
    InvalidTree,
}