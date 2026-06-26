pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;


use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;

declare_id!("4ACXJjwsV5gWRFZTzS7AYpEtf8r977RrD8QEZQgwP39x");

#[program]
pub mod solana_nft_marketplace {
    use super::*;

    //Intializes a new NFT collection with a config PDA.
    //called once per collection by the creator of the collection.
    pub fn initialize_collection(ctx: Context<IntializeCollection>, royalty_bps: u16) -> Result<()> {
        require!(royalty_bps <= 10_000, MarketPlaceError::InvalidRoyalty);

        let config = &mut ctx.accounts.collection_config;

        config.authority = ctx.accounts.authority.key();
        config.royalty_bps = royalty_bps;
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
}


#[derive(Accounts)]
pub struct IntializeCollection<'info>{
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = 8 + CollectionConfig::INIT_SPACE,
        seeds =  [b"config", authority.key().as_ref()],
        bump

    )]
    pub collection_config: Account<'info, CollectionConfig>,

    pub system_program: Program<'info, System>

}

#[account]
#[derive(InitSpace)]
pub struct CollectionConfig{
    //admin
    pub authority: Pubkey,
    //Royalty in basis points - 500 = 5%
    pub royalty_bps: u16,
    //merkle tree holding all cNFTs in this collection
    pub tree_address: Pubkey,
    pub total_minted: u64,
    pub is_active: bool,
    pub bump: u8
}


#[error_code]
pub enum MarketPlaceError{
    #[msg("Royalty basis basis points cannot exceed 10,000(100%)")]
    InvalidRoyalty
}