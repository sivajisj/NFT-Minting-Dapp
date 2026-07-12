import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
} from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import { Program, AnchorProvider, Wallet } from "@coral-xyz/anchor";
import {
  createAllocTreeIx,
  ValidDepthSizePair,
} from "@solana/spl-account-compression";
import fs from "fs";
import path from "path";

async function setupTree() {
  const connection = new Connection(
    "https://api.devnet.solana.com",
    "confirmed"
  );
  const authority = Keypair.fromSecretKey(
    Buffer.from(
      JSON.parse(
        fs.readFileSync(
          path.resolve(process.env.HOME!, ".config/solana/id.json"),
          "utf-8"
        )
      )
    )
  );

  const wallet = new Wallet(authority);
  const provider = new AnchorProvider(connection, wallet, {
    commitment: "confirmed",
  });
  anchor.setProvider(provider);

  const idl = JSON.parse(
    fs.readFileSync(
      path.resolve("target/idl/solana_nft_marketplace.json"),
      "utf-8"
    )
  );
  const program = new Program(idl, provider);

  // Generate a new keypair for the Merkle tree account
  const merkleTreeKeypair = Keypair.generate();

  console.log("Merkle tree address:", merkleTreeKeypair.publicKey.toBase58());

  // Tree parameters — small for devnet testing
  const maxDepth = 14;
  const maxBufferSize = 64;
  const canopyDepth = 10;

  const depthSizePair: ValidDepthSizePair = {
    maxDepth,
    maxBufferSize,
  };

  // Step 1: Pre-allocate the tree account with correct size
  // SPL Compression requires the account to be allocated before init
  const allocTreeIx = await createAllocTreeIx(
    connection,
    merkleTreeKeypair.publicKey,
    authority.publicKey,
    depthSizePair,
    canopyDepth
  );

  // Derive CollectionConfig PDA
  const [collectionConfigPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("config"), authority.publicKey.toBuffer()],
    program.programId
  );

  const BUBBLEGUM_PROGRAM_ID = new PublicKey(
    "BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY"
  );
  const SPL_NOOP_PROGRAM_ID = new PublicKey(
    "noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV"
  );
  const SPL_COMPRESSION_PROGRAM_ID = new PublicKey(
    "cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK"
  );

  // Tree config PDA
  const [treeConfig] = PublicKey.findProgramAddressSync(
    [merkleTreeKeypair.publicKey.toBuffer()],
    BUBBLEGUM_PROGRAM_ID
  );

  // Step 2: Call create_tree instruction
  const createTreeIx = await program.methods
    .createTree(maxDepth, maxBufferSize)
    .accounts({
      authority: authority.publicKey,
      treeConfig,
      merkleTree: merkleTreeKeypair.publicKey,
      collectionConfig: collectionConfigPDA,
      bubblegumProgram: BUBBLEGUM_PROGRAM_ID,
      logWrapper: SPL_NOOP_PROGRAM_ID,
      compressionProgram: SPL_COMPRESSION_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    })
    .instruction();

  // Send both instructions in one transaction
  const tx = new anchor.web3.Transaction();
  tx.add(allocTreeIx);
  tx.add(createTreeIx);

  const sig = await anchor.web3.sendAndConfirmTransaction(
    connection,
    tx,
    [authority, merkleTreeKeypair], // both must sign
    { commitment: "confirmed" }
  );

  console.log("Tree created! Signature:", sig);
  console.log("Save this tree address:", merkleTreeKeypair.publicKey.toBase58());
  console.log("Update batch-mint.ts with this address");
}

setupTree().catch(console.error);