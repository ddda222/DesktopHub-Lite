/**
 * 翻译服务商预设。
 *
 * 这些只是「界面上可选的常见兼容接口地址」，不是秘密、也不是运行时能力：
 * 真正的请求、密钥与凭据由 Rust 在阶段 4 接通，界面只负责选择与展示。
 * 放在前端常量里是为了避免为纯展示数据新增一条宿主命令。
 */
import type { TranslationProvider } from '@/lib/desktop/types'

export const translationProviderPresets: TranslationProvider[] = [
  {
    id: 'deepseek',
    name: 'DeepSeek',
    baseUrl: 'https://api.deepseek.com/v1',
    model: 'deepseek-chat'
  },
  {
    id: 'ark',
    name: '火山方舟',
    baseUrl: 'https://ark.cn-beijing.volces.com/api/v3',
    model: '',
    note: '模型名按控制台接入点填写'
  },
  {
    id: 'qwen',
    name: '通义千问',
    baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    model: 'qwen-plus'
  },
  {
    id: 'kimi',
    name: 'Kimi',
    baseUrl: 'https://api.moonshot.cn/v1',
    model: 'moonshot-v1-8k'
  },
  {
    id: 'siliconflow',
    name: '硅基流动',
    baseUrl: 'https://api.siliconflow.cn/v1',
    model: 'Qwen/Qwen2.5-7B-Instruct'
  },
  {
    id: 'openai',
    name: 'OpenAI',
    baseUrl: 'https://api.openai.com/v1',
    model: 'gpt-4o-mini'
  },
  {
    id: 'custom',
    name: '自定义兼容接口',
    baseUrl: 'http://127.0.0.1:11434/v1',
    model: '',
    note: '任何 OpenAI 兼容接口'
  }
]

/** 服务商 id 不在预设里时（用户填了自定义地址）也不能让界面空掉。 */
export function findProvider(id: string): TranslationProvider {
  return (
    translationProviderPresets.find((item) => item.id === id) ??
    translationProviderPresets[translationProviderPresets.length - 1]
  )
}
