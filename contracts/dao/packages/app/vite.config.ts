import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { nodePolyfills } from "vite-plugin-node-polyfills";

export default defineConfig({
  plugins: [
    react(),
    nodePolyfills({
      include: ["buffer", "crypto", "stream", "util", "events", "process"],
      globals: {
        Buffer: true,
        global: true,
        process: true,
      },
      protocolImports: true,
    }),
  ],
  define: {
    "process.env": {},
    global: "globalThis",
  },
  resolve: {
    alias: {
      "@": "/src",
    },
  },
  build: {
    commonjsOptions: {
      transformMixedEsModules: true,
    },
    // Increase chunk size warning limit for Solana/crypto libraries
    chunkSizeWarningLimit: 1000, // 1MB
    rollupOptions: {
      onwarn(warning, warn) {
        // Suppress specific warning about unresolved polyfill shims
        if (warning.message?.includes("vite-plugin-node-polyfills/shims")) {
          return;
        }
        warn(warning);
      },
      output: {
        // Manual chunk splitting for better caching
        manualChunks: {
          // Solana/crypto libraries (rarely change)
          solana: ["@solana/web3.js", "@coral-xyz/anchor", "bn.js"],
          // Wallet adapters
          wallets: ["@solana/wallet-adapter-react", "@solana/wallet-adapter-base"],
          // React core
          react: ["react", "react-dom", "react-router-dom"],
        },
      },
    },
  },
  optimizeDeps: {
    include: ["@aegis/dao-sdk", "bn.js", "@coral-xyz/anchor", "@solana/web3.js"],
    esbuildOptions: {
      target: "esnext",
      define: {
        global: "globalThis",
      },
    },
  },
});
