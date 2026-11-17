export const islandStyles = [
  { id: 'orbit', name: '轨道卫星', tag: '环绕 · 聚合', shape: '●', description: '功能入口围绕胶囊散开，选中一个，卫星归位，面板展开。', motion: '入口环绕散开 → 选择 → 归位展开', action: '唤出卫星' },
  { id: 'magnet', name: '磁吸积木', tag: '分离 · 吸附', shape: '▰', description: '时间、任务和工具吸成一体。展开时模块松开，收起时弹性吸回。', motion: '模块分离 → 内容就位 → 弹性吸回', action: '弹开模块' },
  { id: 'shutter', name: '机械滑盖', tag: '滑动 · 精密', shape: '▤', description: '带刻度的外壳向两侧滑开，工具像抽屉一样切换。', motion: '滑盖开启 → 工具匣抽出 → 合拢', action: '抽换工具匣' },
  { id: 'ribbon', name: '折叠纸带', tag: '折叠 · 层次', shape: '◩', description: '一条厚纸带展开成标签，再露出内容。折痕留在边缘，正文保持正面。', motion: '细线 → 折页标签 → 内容展开', action: '展开纸带' },
  { id: 'ink', name: '墨滴生长', tag: '生长 · 吸附', shape: '●', description: '墨滴轮廓向两侧生长。接收文件时轻微形变，文字始终稳定。', motion: '轮廓生长 → 文件吸附 → 回弹', action: '模拟文件吸附' },
  { id: 'light', name: '光轨舱门', tag: '巡行 · 分流', shape: '⌑', description: '光沿边缘巡行，展开时向四角分流，像一扇轻轻打开的舱门。', motion: '边缘巡光 → 四角分流 → 面板打开', action: '重播舱门光轨' },
  { id: 'vinyl', name: '黑胶唱片', tag: '旋转 · 封套', shape: '◉', description: '圆盘与状态并排，展开后形成唱片封套式分区。', motion: '唱片旋转 → 封套滑出 → 唱片归位', action: '播放 / 暂停唱片' },
  { id: 'pixel', name: '像素掌机', tag: '跳格 · 反馈', shape: '✚', description: '像素外壳与小型状态灯，完成时播放一段像素反馈；正文用清晰字体。', motion: '逐格展开 → 状态反馈 → 逐格收回', action: '完成像素任务' },
  { id: 'space', name: '航天仪表', tag: '扫描 · 秩序', shape: '⊕', description: '飞行状态栏展开成简洁仪表。刻度和扫描只出现在装饰区。', motion: '仪表自检 → 局部扫描 → 工具就位', action: '运行仪表自检' },
  { id: 'stage', name: '微缩舞台', tag: '升起 · 视差', shape: '▱', description: '工具从浅托盘升起。移动指针时装饰层产生轻微视差，正文保持正面。', motion: '托盘升起 → 工具登场 → 轻微视差', action: '升起工具' }
] as const
export type StyleId = typeof islandStyles[number]['id']
