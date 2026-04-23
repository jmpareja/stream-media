import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

const gatewayUrl = process.env.VITE_GATEWAY_URL ?? 'http://127.0.0.1:3000'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': { target: gatewayUrl, changeOrigin: true },
      '/stream': { target: gatewayUrl, changeOrigin: true },
      '/upload': { target: gatewayUrl, changeOrigin: true },
      '/transcode': { target: gatewayUrl, changeOrigin: true },
    },
  },
})
