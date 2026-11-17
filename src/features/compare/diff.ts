export interface DiffLine { left: string; right: string; kind: 'same' | 'changed' | 'added' | 'removed' }
/** 行级 LCS 对齐，限制矩阵大小，避免粘贴大段文本阻塞界面。 */
export function diffLines(left: string, right: string): DiffLine[] {
  const a = left.replace(/\r\n?/g, '\n').split('\n')
  const b = right.replace(/\r\n?/g, '\n').split('\n')
  if (a.length * b.length > 250_000) return Array.from({ length: Math.max(a.length, b.length) }, (_, i) => ({ left: a[i] ?? '', right: b[i] ?? '', kind: a[i] === b[i] ? 'same' : a[i] === undefined ? 'added' : b[i] === undefined ? 'removed' : 'changed' }))
  const table = Array.from({ length: a.length + 1 }, () => new Uint16Array(b.length + 1))
  for (let i = a.length - 1; i >= 0; i--) for (let j = b.length - 1; j >= 0; j--) table[i]![j] = a[i] === b[j] ? table[i + 1]![j + 1]! + 1 : Math.max(table[i + 1]![j]!, table[i]![j + 1]!)
  const result: DiffLine[] = []
  let i = 0, j = 0
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) { result.push({ left: a[i++]!, right: b[j++]!, kind: 'same' }); continue }
    const removed: string[] = [], added: string[] = []
    while ((i < a.length || j < b.length) && !(i < a.length && j < b.length && a[i] === b[j])) {
      if (i < a.length && (j >= b.length || table[i + 1]![j]! >= table[i]![j + 1]!)) removed.push(a[i++]!)
      else added.push(b[j++]!)
    }
    for (let k = 0; k < Math.max(removed.length, added.length); k++) result.push({ left: removed[k] ?? '', right: added[k] ?? '', kind: k >= removed.length ? 'added' : k >= added.length ? 'removed' : 'changed' })
  }
  return result
}
