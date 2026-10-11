import { defineConfig } from "vite";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "es2022",
    rollupOptions: {
      // Three entry points: the panel (index.html), the Side View (chat.html) and the settings
      // window (settings.html, MOE-0029)
      input: { main: "index.html", chat: "chat.html", settings: "settings.html" },
    },
  },
});
