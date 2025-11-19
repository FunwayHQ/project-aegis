import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AegisToken } from "../target/types/aegis_token";
import {
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountInstruction,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID
} from "@solana/spl-token";
import { expect } from "chai";

describe("aegis-token", () => {
  // Configure the client to use the local cluster
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.AegisToken as Program<AegisToken>;
  const payer = provider.wallet;

  let mintKeypair: anchor.web3.Keypair;
  let mintAuthority: anchor.web3.Keypair;
  let userTokenAccount: anchor.web3.PublicKey;

  // Helper function to create associated token account
  async function createTokenAccount(mint: anchor.web3.PublicKey, owner: anchor.web3.PublicKey): Promise<anchor.web3.PublicKey> {
    const ata = getAssociatedTokenAddressSync(mint, owner);

    const ix = createAssociatedTokenAccountInstruction(
      payer.publicKey,
      ata,
      owner,
      mint,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    );

    const tx = new anchor.web3.Transaction().add(ix);
    await provider.sendAndConfirm(tx);

    return ata;
  }

  before(async () => {
    // Generate keypairs for testing
    mintKeypair = anchor.web3.Keypair.generate();
    // Use the payer's keypair as mint authority to avoid airdrop limits
    mintAuthority = (payer as any).payer;
  });

  it("Initializes the AEGIS token mint", async () => {
    const decimals = 9;

    await program.methods
      .initializeMint(decimals)
      .accounts({
        mint: mintKeypair.publicKey,
        mintAuthority: mintAuthority.publicKey,
        payer: payer.publicKey,
      })
      .signers([mintKeypair])
      .rpc();

    // Fetch the mint account
    const mintAccount = await program.provider.connection.getParsedAccountInfo(
      mintKeypair.publicKey
    );

    expect(mintAccount.value).to.not.be.null;
    const mintData = (mintAccount.value.data as any).parsed.info;
    expect(mintData.decimals).to.equal(decimals);
    expect(mintData.mintAuthority).to.equal(mintAuthority.publicKey.toString());
  });

  it("Mints tokens to a user account", async () => {
    const mintAmount = new anchor.BN(1_000_000_000_000_000); // 1 million tokens with 9 decimals

    // Create associated token account
    userTokenAccount = await createTokenAccount(mintKeypair.publicKey, payer.publicKey);

    await program.methods
      .mintTo(mintAmount)
      .accounts({
        mint: mintKeypair.publicKey,
        to: userTokenAccount,
        authority: mintAuthority.publicKey,
      })
      .rpc();

    // Verify the token account balance
    const tokenAccount = await program.provider.connection.getTokenAccountBalance(
      userTokenAccount
    );

    expect(tokenAccount.value.amount).to.equal(mintAmount.toString());
  });

  it("Transfers tokens between accounts", async () => {
    const transferAmount = new anchor.BN(100_000_000_000); // 100 tokens with 9 decimals

    // Create a second user's token account
    const recipient = anchor.web3.Keypair.generate();
    const recipientTokenAccount = await createTokenAccount(mintKeypair.publicKey, recipient.publicKey);

    // Transfer tokens
    await program.methods
      .transferTokens(transferAmount)
      .accounts({
        from: userTokenAccount,
        to: recipientTokenAccount,
        authority: payer.publicKey,
      })
      .rpc();

    // Verify balances
    const fromBalance = await program.provider.connection.getTokenAccountBalance(
      userTokenAccount
    );
    const toBalance = await program.provider.connection.getTokenAccountBalance(
      recipientTokenAccount
    );

    expect(toBalance.value.amount).to.equal(transferAmount.toString());
  });

  it("Burns tokens", async () => {
    const burnAmount = new anchor.BN(50_000_000_000); // 50 tokens with 9 decimals

    // Ensure token account exists (in case previous test failed)
    if (!userTokenAccount) {
      userTokenAccount = await createTokenAccount(mintKeypair.publicKey, payer.publicKey);
      // Mint some tokens first
      await program.methods
        .mintTo(new anchor.BN(100_000_000_000))
        .accounts({
          mint: mintKeypair.publicKey,
          to: userTokenAccount,
          authority: mintAuthority.publicKey,
        })
        .rpc();
    }

    const beforeBurnBalance = await program.provider.connection.getTokenAccountBalance(
      userTokenAccount
    );

    await program.methods
      .burnTokens(burnAmount)
      .accounts({
        mint: mintKeypair.publicKey,
        from: userTokenAccount,
        authority: payer.publicKey,
      })
      .rpc();

    const afterBurnBalance = await program.provider.connection.getTokenAccountBalance(
      userTokenAccount
    );

    const expectedBalance = new anchor.BN(beforeBurnBalance.value.amount)
      .sub(burnAmount)
      .toString();

    expect(afterBurnBalance.value.amount).to.equal(expectedBalance);
  });

  it("Fails to mint beyond total supply cap", async () => {
    const TOTAL_SUPPLY = new anchor.BN("1000000000000000000"); // 1 billion with 9 decimals
    const excessiveAmount = TOTAL_SUPPLY.add(new anchor.BN(1));

    try {
      await program.methods
        .mintTo(excessiveAmount)
        .accounts({
          mint: mintKeypair.publicKey,
          to: userTokenAccount,
          authority: mintAuthority.publicKey,
        })
        .rpc();

      expect.fail("Should have thrown an error for exceeding supply cap");
    } catch (error) {
      // Program should reject excessive minting
      expect(error).to.exist;
    }
  });

  it("Fails to mint with invalid decimals", async () => {
    const invalidDecimals = 6; // Must be 9
    const newMintKeypair = anchor.web3.Keypair.generate();

    try {
      await program.methods
        .initializeMint(invalidDecimals)
        .accounts({
          mint: newMintKeypair.publicKey,
          mintAuthority: mintAuthority.publicKey,
          payer: payer.publicKey,
        })
        .signers([newMintKeypair])
        .rpc();

      expect.fail("Should have thrown an error for invalid decimals");
    } catch (error) {
      // Program should reject invalid decimals
      expect(error).to.exist;
    }
  });
});
