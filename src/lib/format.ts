import type { ClipItem } from '@/lib/desktop/types'

export function relativeTime(timestamp: number): string {
  const diff = Date.now() - timestamp
  const minute = 60_000
  const hour = 60 * minute
  const day = 24 * hour

  if (diff < minute) return '刚刚'
  if (diff < hour) return `${Math.floor(diff / minute)} 分钟前`
  if (diff < day) return `${Math.floor(diff / hour)} 小时前`
  if (diff < 7 * day) return `${Math.floor(diff / day)} 天前`

  const date = new Date(timestamp)
  return `${date.getMonth() + 1} 月 ${date.getDate()} 日`
}

/** 主列表里的一行摘要：文本压成单行，图片给尺寸描述 */
export function clipSummary(clip: ClipItem): string {
  if (clip.kind === 'image') {
    if (clip.imageMissing) return '图片文件已缺失'
    return `图片 ${clip.width ?? 0} × ${clip.height ?? 0}`
  }
  return (clip.text ?? '').replace(/\s+/g, ' ').trim() || '（空白文本）'
}

export function clipTypeLabel(clip: ClipItem): string {
  if (clip.kind === 'image') return '图片'
  const text = (clip.text ?? '').trim()
  if (/^https?:\/\//i.test(text)) return '网址'
  return '文本'
}

export function charCount(text: string): number {
  return [...text].length
}
