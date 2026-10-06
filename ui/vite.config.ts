import { defineConfig } from "vite";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "es2022",
    rollupOptions: {
      // Two entry points: the panel (index.html) and the Side View (chat.html)
      input: { main: "index.html", chat: "chat.html" },
    },
  },
});
