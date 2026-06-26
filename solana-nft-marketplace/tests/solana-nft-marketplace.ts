
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolanaNftMarketplace } from "../target/types/solana_nft_marketplace";
import { PublicKey } from "@solana/web3.js";
import { assert } from "chai";

describe("solana-nft-marketplace", () => {
  // Configure the client to use the local cluster
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .SolanaNftMarketplace as Program<SolanaNftMarketplace>;

  // The authority is our local wallet
  const authority = provider.wallet as anchor.Wallet;

  // Derive the CollectionConfig PDA — same seeds as the program
  const [collectionConfigPDA, bump] = PublicKey.findProgramAddressSync(
    [Buffer.from("config"), authority.publicKey.toBuffer()],
    program.programId
  );

  it("Initializes a collection with valid royalty", async () => {
    const royaltyBps = 500; // 5%

    await program.methods
      .initializeCollection(royaltyBps)
      .accounts({
        authority: authority.publicKey,
        collectionConfig: collectionConfigPDA,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Fetch the created account and verify state
    const config = await program.account.collectionConfig.fetch(
      collectionConfigPDA
    );

    assert.equal(
      config.authority.toString(),
      authority.publicKey.toString(),
      "Authority should match wallet"
    );
    assert.equal(config.royaltyBps, 500, "Royalty should be 500 bps");
    assert.equal(config.totalMinted.toNumber(), 0, "Total minted should be 0");
    assert.equal(config.isActive, true, "Collection should be active");
    assert.equal(config.bump, bump, "Bump should match derived bump");
  });

  it("Rejects royalty above 10,000 bps", async () => {
    try {
      await program.methods
        .initializeCollection(10_001) // invalid — above 100%
        .accounts({
          authority: authority.publicKey,
          collectionConfig: collectionConfigPDA,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();

      assert.fail("Should have thrown InvalidRoyalty error");
    } catch (err: any) {
      assert.include(err.message, "InvalidRoyalty");
    }
  });
});
