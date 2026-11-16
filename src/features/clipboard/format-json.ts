/** 只重排 JSON 标记之间的空白，保留长整数、重复字段、字段顺序和字符串转义。 */
export function formatJson(source: string): string {
  JSON.parse(source)
  const tokens = source.match(/"(?:[^"\\]|\\.)*"|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|true|false|null|[{}\[\],:]/g) ?? []
  const output: string[] = []
  let depth = 0
  const newline = () => output.push('\n', '  '.repeat(depth))

  for (let index = 0; index < tokens.length; index++) {
    const token = tokens[index]
    if (token === '{' || token === '[') {
      output.push(token)
      if (tokens[index + 1] !== (token === '{' ? '}' : ']')) {
        depth++
        newline()
      }
    } else if (token === '}' || token === ']') {
      if (tokens[index - 1] !== (token === '}' ? '{' : '[')) {
        depth--
        newline()
      }
      output.push(token)
    } else if (token === ',') {
      output.push(token)
      newline()
    } else {
      output.push(token === ':' ? ': ' : token)
    }
  }
  return output.join('')
}
