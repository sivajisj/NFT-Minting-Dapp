import fetch from "node-fetch";

const HELIUS_RPC_URL = `https://devnet.helius-rpc.com/?api-key=${process.env.HELIUS_API_KEY}`;

// ── Types ──────────────────────────────────────────────────────────

interface AssetProof {
  root: string;
  proof: string[];
  node_index: number;
  leaf: string;
  tree_id: string;
}

interface Asset {
  id: string;
  ownership: {
    owner: string;
    delegate: string | null;
  };
  content: {
    metadata: { name: string; symbol: string };
    links: { image: string };
    json_uri: string;
  };
  royalty: { basis_points: number };
  compression: {
    compressed: boolean;
    data_hash: string;
    creator_hash: string;
    asset_hash: string;
    tree: string;
    seq: number;
    leaf_id: number;
  };
}

// ── DAS Client ─────────────────────────────────────────────────────

export class DasClient {
  private rpcUrl: string;

  constructor(rpcUrl: string) {
    this.rpcUrl = rpcUrl;
  }

  private async rpcCall(method: string, params: object): Promise<any> {
    const response = await fetch(this.rpcUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: method,
        method,
        params,
      }),
    });

    const data = await response.json() as any;

    if (data.error) {
      throw new Error(`DAS API error: ${JSON.stringify(data.error)}`);
    }

    return data.result;
  }

  // Fetch single cNFT metadata
  async getAsset(assetId: string): Promise<Asset> {
    return this.rpcCall("getAsset", { id: assetId });
  }

  // Fetch Merkle proof for transfer/burn
  async getAssetProof(assetId: string): Promise<AssetProof> {
    return this.rpcCall("getAssetProof", { id: assetId });
  }

  // Fetch all cNFTs owned by a wallet
  async getAssetsByOwner(
    ownerAddress: string,
    page = 1,
    limit = 100
  ): Promise<{ items: Asset[]; total: number }> {
    return this.rpcCall("getAssetsByOwner", {
      ownerAddress,
      page,
      limit,
    });
  }

  // Fetch all cNFTs in a collection
  async getAssetsByCollection(
    collectionMint: string,
    page = 1,
    limit = 100
  ): Promise<{ items: Asset[]; total: number }> {
    return this.rpcCall("getAssetsByGroup", {
      groupKey: "collection",
      groupValue: collectionMint,
      page,
      limit,
    });
  }

  // Verify a cNFT exists and is owned by expected wallet
  async verifyOwnership(
    assetId: string,
    expectedOwner: string
  ): Promise<boolean> {
    const asset = await this.getAsset(assetId);
    return asset.ownership.owner === expectedOwner;
  }

  // Get floor price from active listings
  // (your indexer handles this — DAS doesn't know about listings)
  async getCollectionStats(collectionMint: string): Promise<{
    total: number;
    compressed: boolean;
  }> {
    const result = await this.getAssetsByCollection(collectionMint, 1, 1);
    return {
      total: result.total,
      compressed: true,
    };
  }
}

// ── Test Script ────────────────────────────────────────────────────

async function testDasClient() {
  const client = new DasClient(HELIUS_RPC_URL);

  const COLLECTION_MINT = "38zqjmtPeDGteueCPKR7nyzwbHmGPmMhgLEnXytakc5w";
  const AUTHORITY = "9QGL3wPd97tiRj6bV7WThNx3eAkqhcTNhpNrgM4nk17D";

  console.log("── Testing DAS Client ──────────────────────────");

  // Test 1: Get all NFTs owned by authority
  console.log("\n1. Fetching NFTs owned by authority...");
  try {
    const owned = await client.getAssetsByOwner(AUTHORITY);
    console.log(`   Found ${owned.total} assets`);

    if (owned.items.length > 0) {
      const first = owned.items[0];
      console.log(`   First asset: ${first.id}`);
      console.log(`   Name: ${first.content.metadata.name}`);
      console.log(`   Compressed: ${first.compression.compressed}`);

      // Test 2: Get proof for first asset
      console.log("\n2. Fetching Merkle proof for first asset...");
      const proof = await client.getAssetProof(first.id);
      console.log(`   Root: ${proof.root.slice(0, 20)}...`);
      console.log(`   Proof depth: ${proof.proof.length} hashes`);
      console.log(`   Leaf index: ${proof.node_index}`);
    }
  } catch (err) {
    console.log(`   No assets found or DAS error: ${err}`);
  }

  // Test 3: Get collection stats
  console.log("\n3. Fetching collection stats...");
  try {
    const stats = await client.getCollectionStats(COLLECTION_MINT);
    console.log(`   Total NFTs in collection: ${stats.total}`);
  } catch (err) {
    console.log(`   Collection stats error: ${err}`);
  }

  console.log("\n── DAS Client Tests Complete ──────────────────");
}

testDasClient().catch(console.error);