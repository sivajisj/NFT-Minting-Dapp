import {
  Connection,
  Keypair,
  PublicKey,
  ComputeBudgetProgram,
  Transaction,
  sendAndConfirmTransaction,
  TransactionInstruction,
} from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import { Program, AnchorProvider, Wallet } from "@coral-xyz/anchor";
import fs from "fs";
import path from "path";

// ── Configuration ──────────────────────────────────────────────────
const RPC_URL = "https://api.devnet.solana.com";
const BATCH_SIZE = 10;        // transactions per batch
const DELAY_MS = 500;         // delay between batches (rate limiting)
const TOTAL_MINTS = 100;      // total cNFTs to mint (use 100 for testing)
const PRIORITY_FEE = 1_000;   // microlamports per compute unit
const COMPUTE_UNITS = 300_000; // per transaction

// ── Load wallet ────────────────────────────────────────────────────
function loadKeypair(): Keypair {
  const keyPath = path.resolve(
    process.env.HOME!,
    ".config/solana/id.json"
  );
  const raw = fs.readFileSync(keyPath, "utf-8");
  return Keypair.fromSecretKey(Buffer.from(JSON.parse(raw)));
}

// ── Sleep utility ──────────────────────────────────────────────────
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// ── Build single mint transaction ──────────────────────────────────
async function buildMintTransaction(
  program: Program,
  authority: Keypair,
  merkleTree: PublicKey,
  collectionMint: PublicKey,
  leafOwner: PublicKey,
  index: number,
  blockhash: string
): Promise<Transaction> {
  const name = `cNFT #${index}`;
  const symbol = "SNFT";
  const uri = `https://gateway.irys.xyz/YOUR_METADATA_URI_${index}`;

  // Compute budget instructions — always first in transaction
  const computeLimitIx = ComputeBudgetProgram.setComputeUnitLimit({
    units: COMPUTE_UNITS,
  });
  const computePriceIx = ComputeBudgetProgram.setComputeUnitPrice({
    microLamports: PRIORITY_FEE,
  });

  // Derive required PDAs
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
  const TOKEN_METADATA_PROGRAM_ID = new PublicKey(
    "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
  );

  // Tree config PDA (owned by Bubblegum)
  const [treeConfig] = PublicKey.findProgramAddressSync(
    [merkleTree.toBuffer()],
    BUBBLEGUM_PROGRAM_ID
  );

  // Bubblegum signer PDA
  const [bubblegumSigner] = PublicKey.findProgramAddressSync(
    [Buffer.from("collection_cpi")],
    BUBBLEGUM_PROGRAM_ID
  );

  // Collection metadata PDA
  const [collectionMetadata] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("metadata"),
      TOKEN_METADATA_PROGRAM_ID.toBuffer(),
      collectionMint.toBuffer(),
    ],
    TOKEN_METADATA_PROGRAM_ID
  );

  // Collection master edition PDA
  const [collectionMasterEdition] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("metadata"),
      TOKEN_METADATA_PROGRAM_ID.toBuffer(),
      collectionMint.toBuffer(),
      Buffer.from("edition"),
    ],
    TOKEN_METADATA_PROGRAM_ID
  );

  // Build the mint_compressed_nft instruction
  const mintIx = await program.methods
    .mintCompressedNft(name, symbol, uri)
    .accounts({
      authority: authority.publicKey,
      leafOwner,
      treeConfig,
      merkleTree,
      collectionMint,
      collectionMetadata,
      collectionMasterEdition,
      bubblegumSigner,
      collectionConfig: collectionConfigPDA,
      bubblegumProgram: BUBBLEGUM_PROGRAM_ID,
      logWrapper: SPL_NOOP_PROGRAM_ID,
      compressionProgram: SPL_COMPRESSION_PROGRAM_ID,
      tokenMetadataProgram: TOKEN_METADATA_PROGRAM_ID,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .instruction();

  // Build transaction with compute budget instructions first
  const tx = new Transaction();
  tx.add(computeLimitIx);
  tx.add(computePriceIx);
  tx.add(mintIx);
  tx.recentBlockhash = blockhash;
  tx.feePayer = authority.publicKey;

  return tx;
}

// ── Main batch mint function ────────────────────────────────────────
async function batchMint() {
  const connection = new Connection(RPC_URL, "confirmed");
  const authority = loadKeypair();
  const wallet = new Wallet(authority);
  const provider = new AnchorProvider(connection, wallet, {
    commitment: "confirmed",
  });
  anchor.setProvider(provider);

  // Load your program IDL
  const idl = JSON.parse(
    fs.readFileSync(
      path.resolve("target/idl/solana_nft_marketplace.json"),
      "utf-8"
    )
  );
  const program = new Program(idl, provider);

  // Replace these with your actual devnet addresses
  const merkleTree = new PublicKey("YOUR_MERKLE_TREE_ADDRESS");
  const collectionMint = new PublicKey(
    "38zqjmtPeDGteueCPKR7nyzwbHmGPmMhgLEnXytakc5w"
  );

  console.log(`Starting batch mint of ${TOTAL_MINTS} cNFTs...`);
  console.log(`Batch size: ${BATCH_SIZE}`);
  console.log(`Priority fee: ${PRIORITY_FEE} microlamports/CU`);

  let successCount = 0;
  let failCount = 0;

  // Process in batches
  for (let i = 0; i < TOTAL_MINTS; i += BATCH_SIZE) {
    const batchEnd = Math.min(i + BATCH_SIZE, TOTAL_MINTS);
    console.log(`\nBatch ${Math.floor(i / BATCH_SIZE) + 1}: minting ${i} to ${batchEnd - 1}`);

    // Get fresh blockhash for each batch
    const { blockhash } = await connection.getLatestBlockhash("confirmed");

    // Build all transactions in this batch
    const txPromises = [];
    for (let j = i; j < batchEnd; j++) {
      txPromises.push(
        buildMintTransaction(
          program,
          authority,
          merkleTree,
          collectionMint,
          authority.publicKey, // mint to authority for testing
          j,
          blockhash
        )
      );
    }

    const transactions = await Promise.all(txPromises);

    // Submit all transactions in batch in parallel
    const sendPromises = transactions.map(async (tx, idx) => {
      try {
        tx.sign(authority);
        const sig = await connection.sendRawTransaction(tx.serialize(), {
          skipPreflight: false,
          preflightCommitment: "confirmed",
        });
        console.log(`  ✅ Mint ${i + idx}: ${sig.slice(0, 8)}...`);
        successCount++;
        return sig;
      } catch (err: any) {
        console.error(`  ❌ Mint ${i + idx} failed: ${err.message}`);
        failCount++;
        return null;
      }
    });

    await Promise.all(sendPromises);

    // Rate limit between batches
    if (batchEnd < TOTAL_MINTS) {
      console.log(`  Waiting ${DELAY_MS}ms before next batch...`);
      await sleep(DELAY_MS);
    }
  }

  console.log(`\n── Batch Mint Complete ──`);
  console.log(`✅ Successful: ${successCount}`);
  console.log(`❌ Failed:     ${failCount}`);
  console.log(`Total:         ${TOTAL_MINTS}`);
}

// ── Run ────────────────────────────────────────────────────────────
batchMint().catch(console.error);