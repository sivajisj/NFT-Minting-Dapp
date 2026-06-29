#[cfg(test)]
mod tests {
    use anchor_lang::{InstructionData, ToAccountMetas};
    use anchor_lang::prelude::Pubkey as APubkey;
    use litesvm::LiteSVM;
    use solana_keypair::Keypair;
    use solana_signer::Signer;
    use solana_instruction::{AccountMeta, Instruction};

    fn program_id() -> APubkey {
        solana_nft_marketplace::id()
    }

    fn system_program_id() -> APubkey {
        APubkey::from([0u8; 32])
    }

    fn setup() -> (LiteSVM, Keypair) {
        let mut svm = LiteSVM::new();
        let authority = Keypair::new();
        let bytes = include_bytes!(
            "../../../target/deploy/solana_nft_marketplace.so"
        );
        svm.add_program(program_id().to_bytes(), bytes);
        svm.airdrop(
            &authority.pubkey().to_bytes().into(),
            5_000_000_000,
        ).unwrap();
        (svm, authority)
    }

    fn derive_config_pda(authority: &APubkey) -> APubkey {
        let (pda, _bump) = APubkey::find_program_address(
            &[b"config", authority.as_ref()],
            &program_id(),
        );
        pda
    }

    fn make_initialize_ix(
        authority_pubkey: APubkey,
        config_pda: APubkey,
        royalty_bps: u16,
    ) -> Instruction {
        let anchor_accounts =
            solana_nft_marketplace::accounts::InitializeCollection {
                authority: authority_pubkey,
                collection_config: config_pda,
                system_program: system_program_id(),
            }
            .to_account_metas(None);

        let accounts: Vec<AccountMeta> = anchor_accounts
            .iter()
            .map(|a| AccountMeta {
                pubkey: a.pubkey.to_bytes().into(),
                is_signer: a.is_signer,
                is_writable: a.is_writable,
            })
            .collect();

        let data =
            solana_nft_marketplace::instruction::InitializeCollection {
                royalty_bps,
            }
            .data();

        Instruction {
            program_id: program_id().to_bytes().into(),
            accounts,
            data,
        }
    }

    fn send(
        svm: &mut LiteSVM,
        ix: Instruction,
        payer: &Keypair,
    ) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
        let blockhash = svm.latest_blockhash();
        let tx = solana_transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[payer],
            blockhash,
        );
        svm.send_transaction(tx)
    }

    #[test]
    fn test_initialize_collection_success() {
        let (mut svm, authority) = setup();
        let authority_apk = APubkey::from(authority.pubkey().to_bytes());
        let config_pda = derive_config_pda(&authority_apk);
        let ix = make_initialize_ix(authority_apk, config_pda, 500);
        let result = send(&mut svm, ix, &authority);
        assert!(
            result.is_ok(),
            "initialize_collection failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_initialize_collection_invalid_royalty() {
        let (mut svm, authority) = setup();
        let authority_apk = APubkey::from(authority.pubkey().to_bytes());
        let config_pda = derive_config_pda(&authority_apk);
        let ix = make_initialize_ix(authority_apk, config_pda, 10_001);
        let result = send(&mut svm, ix, &authority);
        assert!(
            result.is_err(),
            "Should have rejected royalty above 10,000 bps"
        );
    }

    #[test]
    fn test_double_initialize_fails() {
        let (mut svm, authority) = setup();
        let authority_apk = APubkey::from(authority.pubkey().to_bytes());
        let config_pda = derive_config_pda(&authority_apk);

        let ix1 = make_initialize_ix(authority_apk, config_pda, 500);
        let result1 = send(&mut svm, ix1, &authority);
        assert!(result1.is_ok(), "First initialize should succeed");

        let ix2 = make_initialize_ix(authority_apk, config_pda, 500);
        let result2 = send(&mut svm, ix2, &authority);
        assert!(
            result2.is_err(),
            "Double initialization must be rejected"
        );
    }

    #[test]
    fn test_collection_config_after_state_init(){
        let (mut svm, authority) = setup();

        let authority_apk = APubkey::from(authority.pubkey().to_bytes());
        let config_pda = derive_config_pda(&authority_apk);


        //intialize with 750 bps
        let ix = make_initialize_ix(authority_apk, config_pda, 750);
        send(&mut svm, ix, &authority).unwrap();

     // Fetch the account and verify every field
    let account = svm.get_account(&config_pda.to_bytes().into()).unwrap();

    // Anchor account data starts after 8-byte discriminator
    // We verify the account exists and has correct size
    assert_eq!(
        account.data.len(),
        116,
        "Account size must match declared SIZE constant"
    );

        // Verify the account is owned by our program
    assert_eq!(
        account.owner.to_bytes(),
        program_id().to_bytes(),
        "Account must be owned by our program"
    );

    }


    #[test]
    fn test_unauthorized_user_cannot_use_another_pda(){
        let( mut svm, authority) = setup();
    let authority_apk = APubkey::from(authority.pubkey().to_bytes());

    let attacker = Keypair::new();
    svm.airdrop(
        &attacker.pubkey().to_bytes().into(),
        5_000_000_000,
    ).unwrap();
    let attacker_apk = APubkey::from(attacker.pubkey().to_bytes());
    // Attacker tries to initialize using AUTHORITY's PDA
    // but signs with their own keypair
    // The PDA seeds include authority.pubkey() so this derives
    // a DIFFERENT PDA than what authority would get
    let authority_config_pda = derive_config_pda(&authority_apk);
    let ix = make_initialize_ix(
        attacker_apk,           // attacker is the signer
        authority_config_pda,   // but tries to use authority's PDA
        500,
    );
    
    let result = send(&mut svm, ix, &attacker);
    assert!(
        result.is_err(),
        "Attacker must not be able to initialize authority's PDA"
    );

    }

    #[test]
    fn test_royalty_boundry_values(){
        //0 bps valid - (no royalty )
    let (mut svm, authority) = setup();

         let authority_apk = APubkey::from(authority.pubkey().to_bytes());
    let config_pda = derive_config_pda(&authority_apk);
    let ix = make_initialize_ix(authority_apk, config_pda, 0);
    assert!(
        send(&mut svm, ix, &authority).is_ok(),
        "0 bps royalty should be valid"
    );
        // 10_000 bps — valid (exactly 100%)
    let (mut svm2, authority2) = setup();
    let authority_apk2 = APubkey::from(authority2.pubkey().to_bytes());
    let config_pda2 = derive_config_pda(&authority_apk2);
    let ix2 = make_initialize_ix(authority_apk2, config_pda2, 10_000);
    assert!(
        send(&mut svm2, ix2, &authority2).is_ok(),
        "10,000 bps royalty should be valid"
    );
        // 10_001 bps — invalid (above 100%)
    let (mut svm3, authority3) = setup();
    let authority_apk3 = APubkey::from(authority3.pubkey().to_bytes());
    let config_pda3 = derive_config_pda(&authority_apk3);
    let ix3 = make_initialize_ix(authority_apk3, config_pda3, 10_001);
    assert!(
        send(&mut svm3, ix3, &authority3).is_err(),
        "10,001 bps royalty must be rejected"
    );

    }



}
