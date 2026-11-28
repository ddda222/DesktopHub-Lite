/** 网页专用内存会话，切换导航不会重置；桌面适配器不会使用。 */
export const mockDevices = [
  { id: 'demo-headphones', name: '蓝牙耳机 (Stereo)', isDefault: true },
  { id: 'demo-speaker', name: '桌面音箱 (Realtek)', isDefault: false },
  { id: 'demo-monitor', name: '显示器音频 (NVIDIA)', isDefault: false }
]
export const mockSessions = ['腾讯会议', '浏览器', '音乐'].map((name, i) => ({ id: `demo-${i}`, name, deviceId: 'demo-headphones', deviceName: '蓝牙耳机 (Stereo)', appKey: `demo-${i}`, pid: 0, volume: [85, 40, 25][i]!, muted: false, active: true }))
export const mockProfiles = [
  { name: '开会', settings: mockSessions.map((s, i) => ({ appKey: s.appKey, deviceId: s.deviceId, volume: i ? 20 : 90, muted: i > 0 })) },
  { name: '游戏', settings: mockSessions.map((s, i) => ({ appKey: s.appKey, deviceId: s.deviceId, volume: [65, 30, 45][i]!, muted: i === 1 })) }
]
