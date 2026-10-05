import { defineConfig } from "vite";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "es2022",
    rollupOptions: {
      // 面板（index.html）与侧栏（chat.html）两个入口
      input: { main: "index.html", chat: "chat.html" },
    },
  },
});
