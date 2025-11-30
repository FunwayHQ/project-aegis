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
    rollupOptions: {
      onwarn(warning, warn) {
        // Suppress specific warning about unresolved polyfill shims
        if (warning.message?.includes("vite-plugin-node-polyfills/shims")) {
          return;
        }
        warn(warning);
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
