import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// Tauri 桌面应用：Vite 仅负责前端构建，所有后端调用走 invoke() IPC
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ['VITE_', 'TAURI_'],
})
