
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

declare_id!("3TphjLz52Xv9a2sW9C56dA31ouXZRkwdaPhhjJZWYjvK");

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

    /// The collection's mint account — new keypair
    #[account(
        init,
        payer = authority,
        mint::decimals = 0,
        mint::authority = authority,
        mint::freeze_authority = authority,
    )]
    pub collection_mint: Account<'info, Mint>,

    /// Metaplex metadata PDA for the collection NFT
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

    /// Authority's ATA for collection mint
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = collection_mint,
        associated_token::authority = authority,
    )]
    pub authority_token_account: Account<'info, TokenAccount>,

    /// Collection config — updated with collection_mint address
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
pub struct MintNft<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    /// Member NFT mint — new keypair
    #[account(
        init,
        payer = creator,
        mint::decimals = 0,
        mint::authority = creator,
        mint::freeze_authority = creator,
    )]
    pub mint: Account<'info, Mint>,

    /// Metaplex metadata PDA for this member NFT
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

    /// Creator's ATA for member NFT
    #[account(
        init_if_needed,
        payer = creator,
        associated_token::mint = mint,
        associated_token::authority = creator,
    )]
    pub creator_token_account: Account<'info, TokenAccount>,

    /// Collection mint — for verification
    /// CHECK: Validated by Metaplex via CPI
    #[account(
        mut,
        constraint = collection_mint.key() == collection_config.collection_mint
            @ MarketplaceError::InvalidCollectionMint,
    )]
    pub collection_mint: UncheckedAccount<'info>,

    /// Collection metadata PDA
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

    /// Collection master edition PDA
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

    /// Collection config
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
    pub authority:        Pubkey,
    pub royalty_bps:      u16,
    pub tree_address:     Pubkey,
    pub collection_mint:  Pubkey,  // ← NEW: stores collection NFT mint
    pub total_minted:     u64,
    pub is_active:        bool,
    pub bump:             u8,
}

impl CollectionConfig {
    // 8 + 32 + 2 + 32 + 32 + 8 + 1 + 1 = 116
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
}