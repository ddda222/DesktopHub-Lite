<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, toRef, watch } from 'vue'
import { diffLines } from './diff'
import { compareDraft, putPreview, type CompareSnapshot as Snapshot } from './state'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'
import { readShelfPreview, refreshShelf, shelf } from '@/features/files/shelf'
const props = withDefaults(defineProps<{ compact?: boolean; floating?: boolean; initial?: Snapshot }>(), { compact: false, floating: false })
const draft = props.initial ? reactive<Snapshot>({ ...props.initial, images: props.initial.images.slice(), texts: props.initial.texts.slice(), names: props.initial.names.slice() }) : compareDraft
const kind = toRef(draft, 'kind')
const mode = toRef(draft, 'mode')
const position = ref(50)
const texts = toRef(draft, 'texts')
const images = toRef(draft, 'images')
const names = toRef(draft, 'names')
const notice = ref('')
const shelfBusy = ref(false)
const aligned = ref<string[]>([])
const dimensions = ref('')
let alignmentVersion = 0
const dialog = ref<HTMLDialogElement | null>(null)
const floatingSnapshot = ref<Snapshot | null>(null)
const pinned = ref(false)
async function openFloating(): Promise<void> {
  const snapshot = { kind: kind.value, mode: mode.value, texts: texts.value.slice(), images: images.value.slice(), names: names.value.slice() }
  error.value = ''
  if (isDesktop) { try { await invokeHost('open_compare_window', { snapshot }); notice.value = '已打开独立对照窗口，可在窗口标题栏置顶。' } catch (e) { error.value = String(e) }; return }
  floatingSnapshot.value = snapshot
  await nextTick(); dialog.value?.showModal()
}
function closeFloating(): void { dialog.value?.close(); floatingSnapshot.value = null; pinned.value = false }
const error = ref('')
const busy = ref(false)
const diffImage = ref('')
const difference = ref<number | null>(null)
const lines = computed(() => diffLines(texts.value[0]!, texts.value[1]!))
const changed = computed(() => lines.value.filter(line => line.kind !== 'same').length)
let generation = 0
const loadVersions = [0, 0]
let disposed = false
onBeforeUnmount(() => { disposed = true; generation++; alignmentVersion++; loadVersions[0] = -1; loadVersions[1] = -1 })
async function fromShelf(event: Event, side: number): Promise<void> {
  const select = event.target as HTMLSelectElement
  const item = shelf.entries.find(item => item.id === select.value); select.value = ''
  if (!item || shelfBusy.value) return
  const ticket = ++loadVersions[side]!
  shelfBusy.value = true; error.value = ''
  try { const preview = await readShelfPreview(item); if (!disposed && ticket === loadVersions[side]) notice.value = putPreview(side, item.name, preview, draft) } catch (e) { if (!disposed && ticket === loadVersions[side]) error.value = String(e) }
  finally { shelfBusy.value = false }
}
async function alignImages(): Promise<void> {
  const ticket = ++alignmentVersion; aligned.value = []; dimensions.value = ''
  if (images.value.some(image => !image)) return
  try {
    const imgs = await Promise.all(images.value.map(decode))
    if (ticket !== alignmentVersion) return
    const w = Math.max(...imgs.map(img => img.naturalWidth)), h = Math.max(...imgs.map(img => img.naturalHeight))
    const scale = Math.min(1, 960 / w, 640 / h)
    aligned.value = imgs.map(img => { const c = document.createElement('canvas'); c.width = Math.max(1, Math.round(w * scale)); c.height = Math.max(1, Math.round(h * scale)); const ctx = c.getContext('2d')!; ctx.fillStyle = '#fff'; ctx.fillRect(0, 0, c.width, c.height); ctx.drawImage(img, 0, 0, img.naturalWidth * scale, img.naturalHeight * scale); return c.toDataURL() })
    dimensions.value = `A ${imgs[0]!.naturalWidth}×${imgs[0]!.naturalHeight} / B ${imgs[1]!.naturalWidth}×${imgs[1]!.naturalHeight} · 左上对齐`
  } catch { if (ticket === alignmentVersion) error.value = '有图片无法解码，请替换后重试。' }
}
watch(() => images.value.slice(), () => { resetDifference(); void alignImages(); if (mode.value === 'diff') void markDifference() })
onMounted(() => { void alignImages(); if (mode.value === 'diff') void markDifference() })
function resetDifference(): void { generation++; diffImage.value = ''; difference.value = null; busy.value = false; error.value = '' }
async function loadFile(event: Event, side: number): Promise<void> {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]; input.value = ''
  if (!file) return
  const ticket = ++loadVersions[side]!
  resetDifference()
  if (file.size > 8 * 1024 * 1024) { error.value = '请选择不超过 8 MiB 的图片。'; return }
  if (!['image/png', 'image/jpeg', 'image/webp', 'image/gif', 'image/bmp'].includes(file.type)) { error.value = '支持 PNG、JPEG、WebP、GIF 和 BMP 图片。'; return }
  try {
    const source = await new Promise<string>((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.onerror = reject; reader.readAsDataURL(file) })
    await decode(source)
    if (ticket !== loadVersions[side]) return
    images.value[side] = source; names.value[side] = file.name
  } catch { if (ticket === loadVersions[side]) error.value = '图片无法解码，请换一张图片。' }
}
function decode(source: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => { const img = new Image(); img.onload = () => resolve(img); img.onerror = reject; img.src = source })
}
async function markDifference(): Promise<void> {
  mode.value = 'diff'
  if (images.value.some(image => !image)) { error.value = '先选择左右两张图片。'; return }
  if (diffImage.value) return
  busy.value = true; error.value = ''
  const ticket = ++generation
  try {
    const [a, b] = await Promise.all(images.value.map(decode))
    if (!a || !b || ticket !== generation) return
    const scale = Math.min(1, 960 / Math.max(a.naturalWidth, b.naturalWidth), 640 / Math.max(a.naturalHeight, b.naturalHeight))
    const width = Math.max(1, Math.round(Math.max(a.naturalWidth, b.naturalWidth) * scale))
    const height = Math.max(1, Math.round(Math.max(a.naturalHeight, b.naturalHeight) * scale))
    const render = (img: HTMLImageElement) => { const c = document.createElement('canvas'); c.width = width; c.height = height; const ctx = c.getContext('2d')!; ctx.fillStyle = '#fff'; ctx.fillRect(0, 0, width, height); ctx.drawImage(img, 0, 0, img.naturalWidth * scale, img.naturalHeight * scale); return ctx }
    const ca = render(a), cb = render(b), da = ca.getImageData(0, 0, width, height), db = cb.getImageData(0, 0, width, height)
    let count = 0
    for (let p = 0; p < da.data.length; p += 4) {
      const delta = Math.max(...[0, 1, 2].map(c => Math.abs(da.data[p + c]! - db.data[p + c]!)))
      if (delta > 24) { da.data[p] = 205; da.data[p + 1] = 53; da.data[p + 2] = 27; count++ }
      else { const grey = Math.round((da.data[p]! + da.data[p + 1]! + da.data[p + 2]!) / 3 * .22 + 199); da.data[p] = grey; da.data[p + 1] = grey; da.data[p + 2] = grey }
    }
    ca.putImageData(da, 0, 0); diffImage.value = ca.canvas.toDataURL(); difference.value = Math.round(count / (width * height) * 1000) / 10
  } catch { if (ticket === generation) error.value = '无法计算图片差异，请检查两张图片。' }
  finally { if (ticket === generation) busy.value = false }
}
function swap(): void { loadVersions[0]!++; loadVersions[1]!++; images.value.reverse(); names.value.reverse(); texts.value.reverse() }
</script>

<template>
  <section class="compare-board" :class="{ compact: props.compact }" aria-label="浮动对照台">
    <div class="compare-toolbar"><div class="segments"><button :aria-pressed="kind === 'image'" @click="kind = 'image'">图片</button><button :aria-pressed="kind === 'text'" @click="kind = 'text'">文字</button></div><div class="compare-actions"><button v-if="!floating" class="subtle" @click="openFloating">浮窗对照 ↗</button><button class="subtle" @click="swap">交换左右 ⇄</button></div></div>
    <div class="shelf-pickers"><label v-for="side in [0, 1]" :key="side"><span>从文件中转放入{{ side ? '右' : '左' }}侧</span><select :disabled="shelfBusy" :aria-label="`从文件中转放入${side ? '右' : '左'}侧`" @focus="refreshShelf" @change="fromShelf($event, side)"><option value="">选择暂存文件</option><option v-for="item in shelf.entries.filter(i => !i.isDir)" :key="item.id" :value="item.id">{{ item.name }}{{ item.available ? '' : '（失效）' }}</option></select></label></div>
    <template v-if="kind === 'image'">
      <div class="compare-labels"><label v-for="(name, i) in names" :key="i"><span>{{ i ? 'B' : 'A' }} · {{ name }}</span><span class="file-action">替换<input type="file" accept="image/png,image/jpeg,image/webp,image/gif,image/bmp" :aria-label="`替换${i ? '右' : '左'}侧图片`" @change="loadFile($event, i)" /></span></label></div>
      <div class="image-comparison" :class="mode">
        <template v-if="mode === 'side'"><template v-for="i in [0, 1]" :key="i"><img v-if="images[i]" :src="images[i]" :alt="`${i ? '右' : '左'}侧对照图片`" @error="error = '图片无法解码，请替换后重试。'" /><p v-else class="image-empty">选择{{ i ? '右' : '左' }}侧图片<br /><small>或从文件中转放入</small></p></template></template>
        <template v-else-if="mode === 'wipe'"><template v-if="aligned.length === 2"><img :src="aligned[1]" alt="右侧对照图片" /><img class="wipe-image" :src="aligned[0]" alt="左侧擦除图层" :style="{ clipPath: `inset(0 ${100 - position}% 0 0)` }" /><span class="wipe-divider" :style="{ left: `${position}%` }"><b>↔</b></span></template><p v-else class="image-empty">先选择两张可解码的图片</p></template>
        <template v-else><img v-if="diffImage" :src="diffImage" alt="差异标记，红色区域为像素差异" /><p v-else role="status">{{ busy ? '正在计算差异…' : '选择两张图片后查看差异' }}</p></template>
      </div>
      <label v-if="mode === 'wipe'" class="wipe-control">擦除位置 <input v-model.number="position" type="range" min="0" max="100" aria-label="图片擦除位置" /><output>{{ position }}%</output></label>
      <p v-if="mode === 'wipe' && dimensions" class="compare-note">{{ dimensions }} · 统一比例，不拉伸图片</p>
      <div class="compare-footer"><div class="segments"><button :aria-pressed="mode === 'side'" @click="mode = 'side'">左右</button><button :aria-pressed="mode === 'wipe'" @click="mode = 'wipe'">滑杆擦除</button><button :aria-pressed="mode === 'diff'" @click="markDifference">差异标记</button></div><small v-if="mode === 'diff' && difference !== null">差异 {{ difference }}%</small></div>
      <p v-if="mode === 'diff'" class="compare-note">红色标出差异 · 左上对齐 · 最长边缩至 960 × 640 内 · 容差 24</p>
    </template>
    <template v-else>
      <div class="text-inputs"><label v-for="(_, i) in texts" :key="i">{{ i ? 'B · 修改后' : 'A · 修改前' }}<textarea v-model="texts[i]" :aria-label="i ? '右侧文字' : '左侧文字'" maxlength="12000" spellcheck="false" @input="loadVersions[i]!++" /></label></div>
      <p class="compare-note" role="status">{{ changed ? `${changed} 行有变化` : '两段文字一致' }} · 行级差异</p>
      <div class="text-diff" aria-label="文字差异"><div v-for="(line, i) in lines" :key="i" :class="line.kind"><span><b>{{ line.kind === 'same' || line.kind === 'added' ? ' ' : '−' }}</b>{{ line.left }}</span><span><b>{{ line.kind === 'same' || line.kind === 'removed' ? ' ' : '+' }}</b>{{ line.right }}</span></div></div>
    </template>
    <p v-if="error" class="compare-error" role="alert">{{ error }}</p>
    <p v-if="notice" class="compare-note" role="status">{{ notice }}</p>
    <dialog v-if="!floating" ref="dialog" class="floating-compare" aria-label="独立对照浮窗演示" @keydown.esc.stop @cancel.prevent="closeFloating" @close="floatingSnapshot = null">
      <header><div><strong>浮动对照台</strong><small>网页浮窗 · 置顶仅展示状态</small></div><div class="compare-actions"><button :aria-pressed="pinned" @click="pinned = !pinned">{{ pinned ? '已置顶（模拟）' : '置顶（模拟）' }}</button><button aria-label="关闭独立对照浮窗" @click="closeFloating">✕</button></div></header>
      <CompareBoard v-if="floatingSnapshot" floating :initial="floatingSnapshot" />
    </dialog>
  </section>
</template>

<style scoped>
.compare-board { min-width: 0; font-size: 12px; color: var(--text); }
.shelf-pickers { display:grid; grid-template-columns:1fr 1fr; gap:12px; margin-bottom:12px; }.shelf-pickers label { min-width:0; font-size:10px; color:var(--text-2); }.shelf-pickers select { display:block; width:100%; margin-top:5px; padding:6px; background:var(--surface-2); border:1px solid var(--line-strong); border-radius:5px; color:var(--text); font:11px var(--font-sans); }.image-empty { min-height:120px; display:grid; place-content:center; text-align:center; color:var(--text-2); font-size:12px; line-height:1.8; }
.compare-toolbar,.compare-footer { display:flex; align-items:center; justify-content:space-between; gap:8px; margin-bottom:12px; flex-wrap:wrap; }
.segments { display:flex; gap:3px; padding:3px; background:var(--surface-2); border:1px solid var(--line); border-radius:8px; }
.segments button { padding:5px 9px; border-radius:5px; font-size:11px; }
.segments button[aria-pressed=true] { background:var(--text); color:var(--surface); }
.subtle { font-size:11px; color:var(--text-2); }
.compare-actions { display:flex; align-items:center; gap:14px; }
.floating-compare { width:900px; max-width:calc(100vw - 24px); max-height:calc(100vh - 24px); overflow:auto; padding:24px; border:1px solid var(--line-strong); border-radius:14px; color:var(--text); background:var(--surface); box-shadow:0 24px 80px #0005; }
.floating-compare::backdrop { background:#10131080; }
.floating-compare > header { display:flex; justify-content:space-between; align-items:center; gap:12px; border-bottom:1px solid var(--line-strong); padding-bottom:16px; margin-bottom:20px; }
.floating-compare > header strong { font-size:14px; }.floating-compare > header small { display:block; margin-top:5px; color:var(--text-2); font-size:10px; }
.floating-compare > header button { font-size:11px; }.floating-compare > header button[aria-pressed=true] { text-decoration:underline; }
.floating-compare :deep(.image-comparison img) { height:350px; }.floating-compare :deep(.image-comparison.side img) { height:260px; }.floating-compare :deep(textarea) { height:180px; }
@media(max-width:550px) { .floating-compare { padding:14px; }.floating-compare :deep(.image-comparison img) { height:220px; }.floating-compare :deep(.image-comparison.side img) { height:180px; } }
.compare-labels,.text-inputs { display:grid; grid-template-columns:1fr 1fr; gap:12px; margin-bottom:8px; }
.compare-labels label { display:flex; min-width:0; justify-content:space-between; gap:6px; font-size:10px; color:var(--text-2); }
.compare-labels label > span:first-child { overflow:hidden; white-space:nowrap; text-overflow:ellipsis; }
.file-action { position:relative; flex:none; text-decoration:underline; cursor:pointer; }
.file-action input { position:absolute; inset:0; opacity:0; width:100%; cursor:pointer; }
.file-action:focus-within { outline:2px solid var(--accent); outline-offset:3px; }
.image-comparison { position:relative; display:grid; grid-template-columns:1fr; overflow:hidden; border:1px solid var(--line-strong); border-radius:8px; background:var(--surface-2); min-height:100px; }
.image-comparison.side { grid-template-columns:1fr 1fr; gap:1px; }
.image-comparison img { display:block; width:100%; height:180px; object-fit:contain; background:var(--surface-2); }
.image-comparison.side img { height:144px; }
.wipe .wipe-image { position:absolute; inset:0; width:100%; height:100%; }
.wipe-divider { position:absolute; top:0; bottom:0; width:2px; background:#202020; pointer-events:none; }
.wipe-divider b { position:absolute; top:50%; left:0; transform:translate(-50%,-50%); background:white; color:#151515; border:1px solid #202020; border-radius:50%; padding:4px; }
.wipe-control { display:flex; align-items:center; gap:8px; margin:10px 0; font-size:11px; }
.wipe-control input { min-width:0; flex:1; accent-color:var(--text); }
.wipe-control output { width:34px; text-align:right; font-variant-numeric:tabular-nums; }
.compare-footer { margin-top:12px; margin-bottom:0; }
.compare-note { color:var(--text-2); font-size:10px; line-height:1.6; margin:8px 0; }
.compare-error { color:var(--danger); font-size:12px; }
.text-inputs label { color:var(--text-2); font-size:11px; min-width:0; }
.text-inputs textarea { display:block; width:100%; height:110px; resize:vertical; margin-top:8px; padding:8px; border:1px solid var(--line-strong); border-radius:6px; background:var(--surface-2); color:var(--text); font:12px/1.7 var(--font-sans); }
.text-diff { max-height:160px; overflow:auto; border:1px solid var(--line); border-radius:6px; }
.text-diff > div { display:grid; grid-template-columns:1fr 1fr; }
.text-diff span { display:flex; gap:5px; padding:4px 6px; overflow-wrap:anywhere; font-size:11px; white-space:pre-wrap; }
.text-diff span + span { border-left:1px solid var(--line-strong); }
.text-diff b { color:var(--text-2); flex:none; }
.text-diff .changed,.text-diff .added,.text-diff .removed { background:var(--surface-3); border-left:3px solid var(--text); }
@media(max-width:420px) { .compare-labels { gap:6px; } .segments button { padding:5px 6px; } .image-comparison.side img { height:110px; } }
</style>
