
use {
    anchor_lang::InstructionData,
    litesvm::LiteSVM,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_keypair::Keypair,
    solana_program::instruction::{Instruction, AccountMeta},
    solana_transaction::versioned::VersionedTransaction,
};

#[test]
fn test_initialize() {
    let program_id = solana_nft_marketplace::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/solana_nft_marketplace.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    
    let authority = Keypair::new();
    svm.airdrop(&authority.pubkey(), 1_000_000_000).unwrap();

    let (collection_config, _bump) = solana_program::pubkey::Pubkey::find_program_address(
        &[b"config", authority.pubkey().as_ref()],
        &program_id,
    );

    let instruction = Instruction::new_with_bytes(
        program_id,
        &solana_nft_marketplace::instruction::InitializeCollection { royalty_bps: 500 }.data(),
        vec![
            AccountMeta::new(authority.pubkey(), true),
            AccountMeta::new(collection_config, false),
            AccountMeta::new_readonly(solana_program::system_program::ID, false),
        ],
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&authority.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&authority]).unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_ok());
}
