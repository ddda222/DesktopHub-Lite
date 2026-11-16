import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// 前端构建产物按用户要求统一放到 D:\codex\DesktopHub-work，不污染仓库目录。
const OUT_DIR = process.env.HUB_LITE_OUT_DIR ?? 'D:/codex/DesktopHub-work/desktophub-lite-dist'

export default defineConfig({
  plugins: [vue()],
  base: './',
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) }
  },
  server: {
    port: 4317,
    strictPort: true,
    // 编辑器/写入工具会临时生成 .xxx.tmpdir 目录，Windows 上监听它们会 EBUSY 崩溃
    watch: { ignored: ['**/.*.tmpdir/**', '**/*.tmp'] }
  },
  build: { outDir: OUT_DIR, emptyOutDir: true, target: 'chrome114' }
})
