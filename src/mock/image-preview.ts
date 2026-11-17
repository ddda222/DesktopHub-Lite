import { shallowRef } from 'vue'

/** 只由浏览器样稿使用，内容来自已生成的合成图片。 */
export const browserImagePreview = shallowRef<{ dataUri: string; name: string } | null>(null)
