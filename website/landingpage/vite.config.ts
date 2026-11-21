import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// A static page: everything is built into dist/ and served as files.
export default defineConfig({
  plugins: [react()],
  // Relative addresses, so the build works at a domain root and under a folder alike.
  base: "./",
  build: { target: "es2022", chunkSizeWarningLimit: 400 },
});
