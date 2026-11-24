# DesktopHub Lite

基于 Vue 3 + TypeScript + Tauri（Rust）构建的 Windows 轻量级桌面灵动岛效率工具，自原 DesktopHub 仓库的 `next/` 目录独立拆分而来，保留完整开发历史。

## 目录与构建说明

- `node_modules/`、`dist/` 与 `src-tauri/target/` 不进入仓库。
- 前端构建产物与本地 Rust 工具链缓存默认指向 `D:\codex\DesktopHub-work`（可通过环境变量 `HUB_LITE_OUT_DIR` 覆盖前端输出目录），避免污染仓库目录与 C 盘。

## 常用命令

- 安装前端依赖：`pnpm install`
- 启动浏览器预览：`pnpm run dev`（默认地址 `http://localhost:4317/`）
- 前端类型检查与构建：`pnpm run build`
- 后端核心离线单测：`pwsh -File scripts/verify-core.ps1`
- 打包 Windows x64 便携版：`pwsh -File scripts/package.ps1`
