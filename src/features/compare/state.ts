import { reactive } from 'vue'
import { isDesktop } from '@/lib/desktop/desktop'
import { sampleImage } from './samples'
import type { FilePreview } from '@/features/files/shelf'

export interface CompareSnapshot { kind: 'image' | 'text'; mode: string; texts: string[]; images: string[]; names: string[] }
export const compareDraft = reactive<CompareSnapshot>({
  kind: 'image', mode: 'side',
  texts: isDesktop ? ['', ''] : ['让工具离你更近一步。\n黑白实底，文字清晰。\n支持图片与文字对照。', '让工具离你更近一步。\n黑白实底，跟随系统主题。\n支持图片与文字对照。\n每一步，都看得清楚。'],
  images: isDesktop ? ['', ''] : [sampleImage(false), sampleImage(true)],
  names: isDesktop ? ['选择左侧图片', '选择右侧图片'] : ['设计稿 · 示例', '实际页面 · 示例']
})
export function putPreview(side: number, name: string, preview: FilePreview, target: CompareSnapshot = compareDraft): string {
  if (preview.kind === 'image' && preview.dataUri) { target.kind = 'image'; target.images[side] = preview.dataUri; target.names[side] = name; }
  else if (preview.kind === 'text' && preview.text !== null) { target.kind = 'text'; target.texts[side] = preview.text.slice(0, 12000); }
  else throw new Error(preview.notice || '当前文件不能放入对照台，请选择图片或文本')
  target.mode = 'side'
  return preview.kind === 'text' && (preview.text?.length ?? 0) > 12000 ? '仅载入前 12000 个字符，原文件未改动' : preview.notice || `已放入${side ? '右' : '左'}侧：${name}`
}
