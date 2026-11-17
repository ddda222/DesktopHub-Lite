import type { FilePreview, ShelfPreviewSample } from '@/features/files/shelf'

/** 仅由网页演示按钮调用；内容在浏览器生成，不读取本机文件。 */
export function makeShelfSamples(): ShelfPreviewSample[] {
  const canvas = document.createElement('canvas')
  canvas.width = 720; canvas.height = 400
  const ctx = canvas.getContext('2d')!
  const gradient = ctx.createLinearGradient(0, 0, 720, 400)
  gradient.addColorStop(0, '#8cc6dd'); gradient.addColorStop(1, '#435b9e')
  ctx.fillStyle = gradient; ctx.fillRect(0, 0, 720, 400)
  ctx.fillStyle = '#ffffff22'; ctx.beginPath(); ctx.arc(585, 70, 160, 0, Math.PI * 2); ctx.fill()
  ctx.fillStyle = '#142337'; ctx.font = '600 38px sans-serif'; ctx.fillText('随手暂存，随时取用。', 46, 226)
  ctx.font = '20px sans-serif'; ctx.fillText('DesktopHub · 图片预览示例', 48, 273)
  const base: FilePreview = { kind: 'info', size: null, modifiedMs: null, text: null, dataUri: null, notice: null }
  const sample = (id: string, name: string, preview: Partial<FilePreview>, isDir = false, available = true): ShelfPreviewSample => ({
    item: { id: `preview-${id}`, name, path: `D:/演示文件/${name}`, isDir, available },
    preview: { ...base, ...preview }
  })
  return [
    sample('text', '本周交付说明.md', { kind: 'text', size: 1248, text: '# 本周交付说明\n\n01 任务状态胶囊\n翻译、文件暂存和记录暂停一眼可见。\n\n02 文件快速预览\n点击文件名或按空格查看，Esc 返回。\n\n03 移出与撤销\n试着移出一项，再按 Ctrl+Z 恢复。\n\n这是一份演示文本，不会读取或修改你的本机文件。' }),
    sample('image', '界面效果.png', { kind: 'image', size: 24576, dataUri: canvas.toDataURL('image/png') }),
    sample('folder', '设计资料', { kind: 'folder', notice: '文件夹显示基本信息；桌面版可打开所在位置查看内容。' }, true),
    sample('pdf', '使用手册.pdf', { size: 286720, notice: '此格式暂不支持内容预览；桌面版可打开所在位置查看。' }),
    sample('missing', '已移动的文件.txt', {}, false, false)
  ]
}
