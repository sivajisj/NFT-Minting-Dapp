import { Connection } from "@solana/web3.js";

async function compareCosts() {
  const connection = new Connection(
    "https://api.devnet.solana.com",
    "confirmed"
  );

  // Rent exemption for standard NFT accounts
  const mintAccountRent = await connection.getMinimumBalanceForRentExemption(82);
  const metadataAccountRent = await connection.getMinimumBalanceForRentExemption(679);
  const tokenAccountRent = await connection.getMinimumBalanceForRentExemption(165);
  const masterEditionRent = await connection.getMinimumBalanceForRentExemption(282);

  const standardNftCost =
    mintAccountRent +
    metadataAccountRent +
    tokenAccountRent +
    masterEditionRent;

  // Merkle tree cost for 10,000 cNFTs
  // max_depth=14, max_buffer_size=64, canopy_depth=10
  const treeSize = 131657; // bytes for these params
  const treeRent = await connection.getMinimumBalanceForRentExemption(treeSize);

  const compressedNftCostPerNft = treeRent / 10_000;

  console.log("── Cost Comparison ──────────────────────────────");
  console.log(`Standard NFT cost:       ${standardNftCost / 1e9} SOL`);
  console.log(`Compressed NFT cost:     ${compressedNftCostPerNft / 1e9} SOL`);
  console.log(
    `Cost reduction:          ${(
      (1 - compressedNftCostPerNft / standardNftCost) *
      100
    ).toFixed(2)}%`
  );
  console.log("");
  console.log("── At Scale (10,000 NFTs) ───────────────────────");
  console.log(
    `10,000 standard NFTs:    ${(standardNftCost * 10_000) / 1e9} SOL`
  );
  console.log(`10,000 compressed NFTs:  ${treeRent / 1e9} SOL`);
  console.log("─────────────────────────────────────────────────");
}

compareCosts().catch(console.error);