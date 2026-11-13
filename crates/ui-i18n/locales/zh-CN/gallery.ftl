## 画廊外壳：导航、壳层，以及各页面的样例文案。
## Gallery chrome: nav, shell, and the per-page sample copy.

gallery-section-controls = 控件
gallery-section-layouts = 布局
gallery-section-materials = 材质
gallery-section-design-systems = 设计体系

gallery-style-md3 = MD3
gallery-style-fluent = Fluent
gallery-style-glass = Glass

gallery-not-implemented = 尚未实现
gallery-appearance-light = 浅色
gallery-appearance-dark = 深色
gallery-sidebar-readout = 语义：on={ $on } 值={ $value }%

## 页面标题（导航行与内容页头）。
## Page titles (nav rows and the content header).

gallery-page-switch-title = 控件 · 开关
gallery-page-checkbox-title = 控件 · 复选框
gallery-page-radio-title = 控件 · 单选框
gallery-page-slider-title = 控件 · 滑块
gallery-page-tabs-title = 控件 · 页签
gallery-page-tooltip-title = 控件 · 提示框
gallery-page-info-bar-title = 控件 · 信息栏
gallery-page-buttons-title = 控件 · 按钮
gallery-page-icon-button-title = 控件 · 图标按钮
gallery-page-fab-menu-title = 控件 · 浮动操作按钮菜单
gallery-page-menu-flyout-title = 控件 · 菜单浮出
gallery-page-combo-title = 控件 · 下拉框
gallery-page-chips-title = 控件 · 芯片
gallery-page-cards-title = 控件 · 卡片
gallery-page-lists-title = 控件 · 列表
gallery-page-icons-title = 控件 · 图标
gallery-page-dialogs-title = 控件 · 对话框
gallery-page-action-sheet-title = 控件 · 动作面板
gallery-page-toast-title = 控件 · 提示条
gallery-page-modal-sheet-title = 控件 · 模态面板
gallery-page-stepper-title = 控件 · 步进器
gallery-page-date-picker-title = 控件 · 日期选择
gallery-page-time-picker-title = 控件 · 时间选择
gallery-page-app-bar-title = 控件 · 应用栏
gallery-page-carousel-title = 控件 · 轮播
gallery-page-text-field-title = 控件 · 文本字段
gallery-page-search-title = 控件 · 搜索
gallery-page-selection-title = 控件 · 文本选区
gallery-page-generic-title = 控件 · 通用包控件
gallery-page-layouts-title = 布局 · 骨架
gallery-page-grid-title = 布局 · 网格
gallery-page-inline-title = 布局 · 行内
gallery-page-markdown-title = 布局 · Markdown
gallery-markdown-edit-title = 布局 · Markdown · 编辑
gallery-markdown-code-numbers = 显示行号
gallery-markdown-code-wrap = 自动换行
gallery-markdown-code-height = 限制高度
gallery-page-math-title = 布局 · 公式
gallery-page-positioning-title = 布局 · 定位
gallery-page-progress-title = 控件 · 进度指示器
gallery-page-segmented-title = 控件 · 分段按钮
gallery-page-nav-bar-title = 控件 · 导航栏
gallery-page-nav-rail-title = 控件 · 导航轨道
gallery-page-nav-drawer-title = 控件 · 导航抽屉
gallery-page-nav-view-title = 控件 · 导航视图
gallery-page-toolbar-title = 控件 · 工具条
gallery-page-snackbar-title = 控件 · 提示条
gallery-page-materials-title = 材质 · 嵌板
gallery-page-md3-system-title = 设计体系 · MD3
gallery-page-fluent-system-title = 设计体系 · Fluent 2
gallery-page-hig-system-title = 设计体系 · HIG

## 开关页。
## Switch.

gallery-switch-caption = { $kind ->
  [icon] 带图标
 *[plain] 无图标
}{ $disabled ->
  [yes] ，已禁用
 *[no] { "" }
}

## 复选框页。
## Checkbox.

gallery-checkbox-caption = { $state ->
  [on] 已选中
  [mixed] 不确定
 *[off] 未选中
}{ $error ->
  [yes] ，错误
 *[no] { "" }
}{ $disabled ->
  [yes] ，已禁用
 *[no] { "" }
}
gallery-checkbox-all-off = 全部取消
gallery-checkbox-all-mixed = 全部不确定
gallery-checkbox-all-on = 全部选中
gallery-checkbox-note = 导出中没有尺寸变体——error 与 disabled 是构建期旋钮
gallery-checkbox-label-off = 未选中
gallery-checkbox-label-on = 已选中
gallery-checkbox-label-mixed = 不确定
gallery-checkbox-label-tri = 全选

## 单选框页。
## Radio.

gallery-radio-option = 选项 { $letter }{ $selected ->
  [yes] （已选中）
 *[no] { "" }
}
gallery-radio-disabled = { $selected ->
  [yes] 已禁用，已选中
 *[no] 已禁用
}
gallery-radio-note = 导出一套几何；selected / disabled 是仅有的旋钮

## 滑块页。
## Slider.

gallery-slider-size-caption = { $size } —— 轨道 { $track }dp、手柄 { $handle }dp
gallery-slider-disabled-caption = 已禁用 —— onSurface 取 12% / 38%
gallery-slider-stepped-caption = 离散 —— { $stops } 个刻度，带刻度点
gallery-slider-range-caption = 范围 —— 两个手柄，{ $start }%..{ $end }%
gallery-slider-range-step-caption = 范围 + 离散 —— 两个手柄都吸附
gallery-slider-readout = 取值 = { $value }%（每行共享）
gallery-slider-value = 取值 = { $value }%

## 页签页。
## Tabs.

gallery-tab-overview = 概览
gallery-tab-specs = 规格
gallery-tab-reviews = 评价
gallery-tab-home = 首页
gallery-tab-search = 搜索
gallery-tab-library = 库
gallery-tab-one = 一
gallery-tab-two = 二
gallery-tab-three = 三
gallery-tab-four = 四
gallery-tab-five = 五
gallery-tabs-caption-text = 纯文字 —— 3 个页签，均分栏宽
gallery-tabs-caption-icons = 带图标 —— 内容 72dp，下划线带 2dp
gallery-tabs-caption-many = 五个页签 —— 份额随数量走
gallery-tabs-caption-disabled = 已禁用 —— onSurface 取 38%，无下划线
gallery-tabs-readout = 已选中：{ $sel }（每栏各一个，点击更换）
gallery-tabs-fluent = WinUI 页签 —— 32dp 页头，选中项并入页面，带关闭与新建按钮

## 提示框页。
## Tooltips.

gallery-tooltip-caption = WinUI 提示框 —— 模糊背景上的丙烯酸，12pt 文本，320px 封顶，167ms 淡入淡出
gallery-tooltip-caption-md3 = Material 3 提示框 —— 反色表面面板，12sp 标签，200dp 封顶，淡入 150ms、淡出 75ms
gallery-tooltip-caption-rich = Material 3 富提示框 —— 320dp 表面，含小标题、说明文字与文本按钮动作；何时显示由应用决定
gallery-tooltip-rich-hint = 点击 Info —— 应用只在打开时声明面板，点击动作关闭
gallery-tooltip-rich-open = 信息
gallery-tooltip-rich-subhead = 富提示框
gallery-tooltip-rich-text = 富提示框就地解释一个功能：小标题点名，短段落说明它做什么，一个动作表示已知悉。
gallery-tooltip-rich-action = 知道了
gallery-tooltip-delay = 悬停按钮 —— 经过 400ms 的系统悬停时间后展开，然后淡入
gallery-tooltip-btn-save = 保存
gallery-tooltip-btn-delete = 删除
gallery-tooltip-btn-long = 长文本
gallery-tooltip-save = 保存文件
gallery-tooltip-delete = 删除选中的行
gallery-tooltip-long = 悬停这个按钮会显示一段足以触及 320px 上限的文字，面板因此换行而不是继续变宽。
gallery-tooltip-long-md3 = 悬停这个按钮会显示一段超过 200dp 上限的标签，面板因此换行而不是继续变宽。

## 按钮页。
## Buttons.

gallery-btn-filled = 填充
gallery-btn-tonal = 音调
gallery-btn-elevated = 立面
gallery-btn-outlined = 描边
gallery-btn-text = 文本
gallery-btn-xs = XS
gallery-btn-small = 小号
gallery-btn-medium = 中号
gallery-btn-large = 大号
gallery-btn-xl = XL
gallery-btn-toggle-a = 切换 A
gallery-btn-toggle-b = 切换 B
gallery-btn-undo = 撤销
gallery-btn-redo = 重做
gallery-btn-copy = 复制
gallery-buttons-caption-styles = 样式 —— 填充、音调、立面（小号）
gallery-buttons-caption-styles2 = 样式 —— 描边、文本：无容器，描边就是可供性
gallery-buttons-caption-sizes = 尺寸 —— 32 / 40 / 56dp（xsmall、small、medium）
gallery-buttons-caption-sizes2 = 尺寸 —— 96 / 136dp，标签随盒子增长
gallery-buttons-caption-icons = 带图标 —— 首内衬、图标、图标-标签间隙、标签
gallery-buttons-caption-toggles = 切换按钮 —— `selected` 归应用，点击翻转
gallery-buttons-caption-group = 连接组 —— 一个胶囊，内侧边缘保持方角
gallery-buttons-caption-disabled = 已禁用 —— 内容 38%，容器 10%
gallery-buttons-caption-disabled2 = 已禁用 —— 无容器，描边保持 12%
gallery-buttons-readout = 切换态：{ $sel }（点击翻转）
gallery-btn-default = 默认
gallery-btn-accent = 强调
gallery-btn-subtle = 弱化
gallery-btn-text-link = 文字链接
gallery-buttons-caption-fluent-styles = WinUI 样式 —— 默认、强调、弱化、文字链接：同一 32dp 行高
gallery-buttons-caption-fluent-border = 立面描边 —— 3dp 渐变，明色主题下上下镜像
gallery-buttons-caption-fluent-disabled = 已禁用 —— 用禁用画刷族，其余不再淡化
gallery-buttons-caption-glass-icons = 玻璃 —— 56dp 胶囊，按住时长边固定长出 17dp
gallery-buttons-caption-glass-labels = 带文字 —— 高 44dp，宽度由文字量出
gallery-buttons-caption-glass-disabled = 已禁用 —— 半透明，按下也不上报
gallery-buttons-glass-readout = 玻璃按下 { $count } 次

## FAB 菜单页。
gallery-split-button-label = 打开
gallery-split-button-caption = 分裂按钮 —— 右半负责菜单，左半负责动作
gallery-split-button-sizes-caption = 尺寸 —— 32 / 40 / 56 / 96 / 136dp，两半共用高度
gallery-split-button-readout = 菜单展开：{ $open } · 左半按下次数：{ $count }

## FAB menu.

gallery-fab-create = 新建
gallery-fab-share = 分享
gallery-fab-edit = 编辑
gallery-fab-note-primary = primary —— 菜单项取 primaryContainer
gallery-fab-note-secondary = secondary —— 菜单项取 secondaryContainer
gallery-fab-note-tertiary = tertiary —— 菜单项取 tertiaryContainer
gallery-fab-note-closed = 收起 —— 只显示锚点
gallery-fab-note-disabled = 已禁用 —— 锚点不接受输入
gallery-fab-tone-primary = primary
gallery-fab-tone-secondary = secondary
gallery-fab-tone-tertiary = tertiary
gallery-fab-readout-picked = 已选：{ $tone } · { $item }（菜单已自行收起）
gallery-fab-readout-none = 已选：还没有（点菜单项，或点锚点收起）

gallery-fab-samples-caption = 尺寸 40 · 56 · 80 · 96 dp，五种配色，一个 lowered
gallery-fab-readout = FAB 激活次数：{ $count }

## 菜单浮出页。
## Menu flyout.

gallery-menu-caption = MenuFlyoutPresenter —— 8dp 圆角、1dp 描边、丙烯酸；条目相距 4dp，内衬 11,8,11,9，图标列 16dp
gallery-menu-show = 显示菜单
gallery-md3-menu-caption = MD3 菜单 —— 4dp 圆角、2dp 内缩；行高 44dp，含分区标题、选中行与禁用行
gallery-md3-menu-section = 文件
gallery-md3-menu-supporting = 打开已保存的文档
gallery-menu-glass-caption = 玻璃菜单 —— 200dp 面板，32dp 圆角，44dp 行、间距 2dp
gallery-menu-new = 新建
gallery-menu-open = 打开
gallery-menu-save = 保存
gallery-menu-share = 分享
gallery-menu-print = 打印
gallery-menu-accel-new = Ctrl+N
gallery-menu-accel-open = Ctrl+O
gallery-menu-accel-save = Ctrl+S
gallery-menu-accel-share = Ctrl+Shift+S
gallery-menu-accel-print = Ctrl+P
gallery-menu-readout-picked = 已执行：{ $command }（菜单已自行收起）
gallery-menu-readout-none = 已执行：还没有（点菜单项，或点锚点收起）
gallery-popover-caption = 玻璃浮出层 —— 菜单那块 280dp 的片，装应用自己的内容块
gallery-popover-show = 展开浮出层
gallery-popover-title = 个人资料
gallery-popover-body = 任何内容块都能放进玻璃里；它们做什么由应用决定。
gallery-popover-done = 完成
gallery-popover-readout = 「完成」按了 { $count } 次（点外面或按 Escape 收起）

## 下拉框页。
## Combo box.

gallery-combo-caption = ComboBox —— 最小 64×32，内衬 12,5,0,7，描边 1dp，圆角 4dp；面板为 8dp 圆角的丙烯酸，宽度跟随选择框
gallery-combo-header = 水果
gallery-combo-placeholder = 选择一种水果
gallery-combo-item-0 = 苹果
gallery-combo-item-1 = 香蕉
gallery-combo-item-2 = 樱桃
gallery-combo-item-3 = 乌李
gallery-combo-item-4 = 接骨木果
gallery-combo-readout-picked = 已选：{ $item }
gallery-combo-readout-none = 已选：还没有（先展开选择框，再点一行）

## 导航视图页。

gallery-nav-view-documents = 文档
gallery-nav-view-search = 搜索
gallery-nav-view-saved = 已保存
gallery-nav-view-print = 打印
gallery-nav-view-body = 窗格宽 320dp；开关把它折成 48dp，标签随之让位，只剩图标与 3×16 的选中指示条。
gallery-nav-view-mode-expanded = 展开
gallery-nav-view-mode-compact = 紧凑
gallery-nav-view-mode-minimal = 精简

## 芯片页。
## Chips.

gallery-chip-assist = 辅助
gallery-chip-suggestion = 建议
gallery-chip-filter = 筛选
gallery-chip-input = 输入
gallery-chip-selected = 已选中
gallery-chip-unselected = 未选中
gallery-chips-caption-kinds = 类型 —— assist / suggestion / filter / input，全为扁平
gallery-chips-caption-icons = 带首标记 —— 内衬从 16 缩到 8
gallery-chips-caption-elevated = 立面 —— 用填充与阴影取代描边环
gallery-chips-caption-selected = 筛选选中 —— 容器取代描边环（点击切换）
gallery-chips-caption-disabled = 已禁用 —— 内容 38%，环与容器 12%
gallery-chips-readout = 筛选选中：{ $sel }
gallery-chips-glass-caption-marks = 玻璃 —— 16dp 标记、6dp 间距、12/8 内衬、14pt w500 标签
gallery-chips-glass-caption-selected = 选中 —— 胶囊内 10% 基准底色（点击切换）
gallery-chips-glass-caption-deletable = 可删除 —— × 从自己的节点上报
gallery-chips-glass-caption-disabled = 已禁用 —— 内容 50%
gallery-chips-glass-deletable = 可移除
gallery-chips-glass-readout = 玻璃选中：{ $sel } · 已移除：{ $gone }

## 卡片页。
## Cards.

gallery-card-elevated = 立面卡片
gallery-card-filled = 填充卡片
gallery-card-outlined = 描边卡片
gallery-card-static = 静态卡片
gallery-card-activated = 已激活 { $count } 次
gallery-card-static-body = 没有动作，因此不绘制状态层
gallery-card-disabled-body = 已禁用
gallery-cards-caption-variants = elevated / filled / outlined —— 点击激活
gallery-cards-caption-static = 静态 —— 不能被悬停或点击，因此不绘制任何层
gallery-cards-caption-disabled = 已禁用 —— 容器 38%，环 12%，无动作
gallery-cards-readout = 激活次数：{ $hits }
gallery-card-glass-title = 玻璃卡片
gallery-card-glass-body = 包默认本体之上，16dp 内衬与 12dp 连续圆角
gallery-cards-glass-caption-card = 玻璃卡片 —— 16dp 内衬、12dp 圆角
gallery-cards-glass-caption-solid = 纯色卡片 —— 浅色为白、深色 #1C1C1E，26dp 圆角
gallery-cards-glass-caption-container = 普通容器 —— 无内衬、16dp 圆角
gallery-cards-glass-note = 本体取 LiquidGlassSettings() 的默认值：厚度 30、磨砂 3、色散 0.5。

## 列表页。
## Lists.

gallery-list-one-line = 单行
gallery-list-two-lines = 两行
gallery-list-three-lines = 三行
gallery-list-supporting = 辅助文本
gallery-list-overline = 上划线
gallery-list-icon = 图标
gallery-list-avatar = 头像
gallery-list-row = 第 { $index } 行
gallery-list-segment = 第 { $index } 段
gallery-list-disabled-row = 禁用行
gallery-lists-caption-lines = 56 / 72 / 88 —— 一行、两行与三行
gallery-lists-caption-leading = 前置槽 —— 图标 24dp、头像 40dp（构建时选定）
gallery-lists-caption-selection = 选中 —— secondary-container 归应用（点击切换）
gallery-lists-caption-segmented = 分段 —— 2dp 间隙与圆角容器
gallery-lists-caption-ornaments = 后置徽标 —— 16dp 胶囊与 6dp 圆点；分隔线 1dp，一条内缩 16
gallery-lists-caption-disabled = 已禁用 —— 内容 38%，无状态层
gallery-list-unread = 收件箱
gallery-list-drafts = 草稿
gallery-lists-glass-caption-group = 卡片中的行 —— 16/12 内衬，分隔线从文字处起
gallery-lists-glass-caption-standalone = 独立行 —— 自带 12dp 分组单元格
gallery-lists-glass-caption-still = 静止行 —— 没有动作的行不画底色
gallery-lists-glass-readout = 行被激活 { $count } 次 —— 按下一行，或用 Tab 移上去
gallery-list-glass-wifi = 无线局域网
gallery-list-glass-wifi-sub = 家庭网络
gallery-list-glass-airplane = 飞行模式
gallery-list-glass-airplane-sub = 关闭
gallery-list-glass-dnd = 勿扰模式
gallery-list-glass-dnd-sub = 至 8:00
gallery-list-glass-standalone = 定位
gallery-list-glass-standalone-sub = 使用 App 期间
gallery-list-glass-still = 版本
gallery-list-glass-still-sub = 26.2

## 图标页。
## Icons.

gallery-icons-column = Material Symbols · { $set }
gallery-icons-caption-toggle = 点击心形以动画 FILL

## 对话框页。
## Dialogs.

gallery-dialog-caption-no-icon = 无图标 —— 每行都从缩进起，两个动作
gallery-dialog-caption-with-icon = 带图标 —— 各行居中，三个动作仍右对齐
gallery-dialog-caption-no-title = 无标题 —— 正文占标题的缩进，一个动作
gallery-dialog-title-discard = 放弃草稿？
gallery-dialog-body-discard = 这份草稿将永久删除。
gallery-dialog-title-reset = 重置设置？
gallery-dialog-body-reset = 所有本地改动都会被移除，包括未保存的。
gallery-dialog-body-undone = 这里没有任何操作可以撤销。
gallery-dialog-title-publish = 发布改动？
gallery-dialog-body-publish = 遮罩之后的一切都会被挡住，直到你做出选择。
gallery-dialog-action-cancel = 取消
gallery-dialog-action-discard = 放弃
gallery-dialog-action-later = 稍后
gallery-dialog-action-save = 保存
gallery-dialog-action-reset = 重置
gallery-dialog-action-ok = 好的
gallery-dialog-show-modal = 显示模态
gallery-dialog-caption-modal = 模态 —— 遮罩覆盖本窗口：点表面之外即取消
gallery-dialog-show-in-panel = 在嵌板中显示
gallery-dialog-panel-note = 可以承载模态的嵌板
gallery-dialog-host-note = 可以承载模态的嵌板 —— 点按钮，遮罩只覆盖这块嵌板
gallery-dialog-column-host = MD3 —— 模态宿主
gallery-dialog-column-fluent = Fluent —— 内容对话框
gallery-dialog-column-glass = LiquidGlass —— 警告框
gallery-dialog-glass-caption-two = 两个动作 —— 破坏性动作沿用系统红
gallery-dialog-glass-caption-three = 三个动作 —— 按钮带竖排，iOS 做法
gallery-dialog-glass-caption-one = 单个动作 —— 独占整宽
gallery-dialog-fluent-caption-default = 主按钮为默认 —— 强调色标出回车键的动作
gallery-dialog-fluent-caption-none = 无默认 —— 三个动作平分命令区
gallery-dialog-fluent-caption-close = 只有关闭 —— 唯一动作留在右半
gallery-dialog-fluent-show = 显示内容对话框
gallery-dialog-action-delete = 删除
gallery-dialog-action-keep = 保留
gallery-dialog-action-publish = 发布
gallery-dialogs-readout = 上次动作：{ $action } · 取消次数：{ $count }
gallery-action-sheet-show = 显示动作面板
gallery-action-sheet-caption = 面板自底缘升起——8dp 内衬，两块卡片
gallery-action-sheet-readout = 上次动作：{ $action } · 取消次数：{ $count }
gallery-action-sheet-title = 分享照片
gallery-action-sheet-message = 选择要对这张照片做什么。
gallery-action-sheet-open = 打开
gallery-action-sheet-share = 分享
gallery-action-sheet-delete = 删除
gallery-action-sheet-cancel = 取消

## 提示条页。
gallery-toast-show = 显示提示条
gallery-toast-show-error = 在顶部显示错误
gallery-toast-caption = 距边缘 16dp 的 48dp 胶囊——它浮着，不挡操作
gallery-toast-readout = 动作：{ $picked } · 关闭：{ $count }
gallery-toast-saved = 照片已存入图库
gallery-toast-undo = 撤销
gallery-toast-failed = 无法连接服务器

## 模态面板页。
gallery-modal-sheet-show = 显示模态面板
gallery-modal-sheet-caption = 半高为窗口的 45%，全高距顶端 90dp——拖动手柄
gallery-modal-sheet-readout = 档位：{ $detent } · 关闭：{ $count }
gallery-modal-sheet-title = 照片详情
gallery-modal-sheet-body = 向上拖手柄到全高，向下拖则关闭。
gallery-modal-sheet-half = 半高
gallery-modal-sheet-full = 全高
gallery-modal-sheet-none = —
gallery-modal-sheet-done = 完成

gallery-bottom-sheet-open = 显示 MD3 底部面板
gallery-bottom-sheet-caption = 模态 · 最宽 640，拖过一半即关闭
gallery-bottom-sheet-readout = 关闭次数：{ $count }
gallery-bottom-sheet-title = MD3 底部面板
gallery-bottom-sheet-body = 将手柄向下拖过一半高度，或点击面板外即可关闭。
gallery-bottom-sheet-done = 完成

gallery-side-sheet-open = 显示 MD3 侧边面板
gallery-side-sheet-caption = 模态 · 256 宽，贴尾缘
gallery-side-sheet-readout = 关闭次数：{ $count }
gallery-side-sheet-title = 侧边面板
gallery-side-sheet-body = 贴尾缘的面板，内缘有一道分隔线。
gallery-side-sheet-save = 保存

## 步进器。
gallery-stepper-caption = 94×34 胶囊；按住一侧 0.5 秒后开始重复，此后每 0.1 秒一次
gallery-stepper-caption-wrap = 越过端点后循环
gallery-stepper-caption-disabled = 禁用
gallery-stepper-readout = 值：{ $value } · 循环：{ $wrap } · 步进：{ $count }

## 日期选择页。
## Date picker.

gallery-date-readout = 今天 { $today } · 已选：{ $picked } · 显示：{ $month } { $year } · 视图：{ $view }
gallery-date-view-days = 日
gallery-date-view-months = 月
gallery-date-view-years = 年
gallery-date-caption-mon = 嵌入式 · 周一开头
gallery-date-caption-sun = 嵌入式 · 周日开头，10–24 日可选
gallery-date-modal-caption = 模态 · 遮罩上的 360×524
gallery-date-modal-open = 打开模态日期选择器
gallery-date-range-caption = 模态 · 一个滚动月份列表上的两个日期
gallery-date-range-open = 打开日期范围选择器
gallery-date-range-readout = 范围：{ $start } – { $end }
gallery-date-input-title = MD3 · 文本输入
gallery-date-input-readout = 缓冲「{ $text }」· 最近动作：{ $action }
gallery-date-input-none = 尚无
gallery-date-input-calendar = 切到日历
gallery-date-input-cancel = 取消
gallery-date-input-confirm = 确定
gallery-date-input-supporting = 选择日期
gallery-date-input-headline = 未选择日期
gallery-date-input-label = 日期
gallery-date-input-placeholder = 年/月/日
gallery-date-input-hint = 输入日期后确认
gallery-date-input-cancel-label = 取消
gallery-date-input-confirm-label = 确定
gallery-date-input-pending = Fluent 与玻璃两套还没有日期选择器。

## 时间选择页。
## Time picker.

gallery-time-caption-live = 时间输入 · 实时
gallery-time-caption-disabled = 时间输入 · 已禁用

## 应用栏页。
## App bar.

gallery-appbar-title-inbox = 收件箱
gallery-appbar-title-medium = Medium
gallery-appbar-title-large = Large
gallery-appbar-title-flexible = Flexible
gallery-appbar-title-search = 搜索
gallery-appbar-sub-medium = 中等可收起 · 带副标题 136dp
gallery-appbar-sub-large = 大号可收起 · display-small 收拢为 title-large
gallery-appbar-caption-small = small · 64dp · 前置图标 + 两个动作
gallery-appbar-caption-medium = medium · 112dp · 前置 32dp 头像，标题在带下方
gallery-appbar-caption-large = large · 152dp · headline-medium 标题在底部
gallery-appbar-caption-medium-flex = medium flexible · 112dp，带副标题 136dp · 收拢到 64dp
gallery-appbar-caption-large-flex = large flexible · 120dp，带副标题 152dp · 收拢到 64dp
gallery-appbar-caption-search = search · 56dp 胶囊 · surface-container 配 on-surface-variant 标签
gallery-appbar-toggle-scrolled = 切换滚入其下
gallery-appbar-toggle-collapsed = 切换收起
gallery-appbar-readout = 上次槽位：{ $slot } · 滚入其下：{ $scrolled } · 收起：{ $collapsed }

## 轮播页。
## Carousel.

gallery-carousel-caption-multi = multi-browse · 16dp 槽宽 · 360dp 下 156 / 104 / 52
gallery-carousel-caption-hero = hero · 280dp 大条目，40dp 条目探入
gallery-carousel-caption-hero-centered = 居中大图 hero · 大条目夹在两个小条目之间
gallery-carousel-caption-uncontained = uncontained · 每个条目 280dp，滑入槽下
gallery-carousel-caption-full = 全屏 · 一次一个 120dp 条目，纵向
gallery-carousel-caption-loop = 循环 · 八个条目，永远走不到尽头
gallery-carousel-prev = 上一个
gallery-carousel-next = 下一个
gallery-carousel-readout = 上次条目：{ $item }
gallery-carousel-offset = 偏移 { $offset } / { $max }dp
gallery-carousel-item = 样本 { $sample } 条目 { $index }

## 文本字段与搜索页。
## Text fields and search.

gallery-tf-filled = 填充
gallery-tf-outlined = 描边
gallery-tf-placeholder = 在此输入
gallery-tf-menu-clear = 清空字段
gallery-tf-readout = 应用缓冲：{ $buffers }
gallery-tf-chord-caption = 快捷键 · 聚焦后按一个组合键
gallery-tf-shortcut-placeholder = 按下组合键
gallery-tf-label = 标签
gallery-tf-name = 姓名
gallery-tf-email = 邮箱
gallery-tf-search = 搜索
gallery-tf-disabled = 已禁用
gallery-tf-supporting = 辅助文本
gallery-tf-error-supporting = 请输入有效邮箱
gallery-tf-caption-fluent = 标题 · 占位符 · 在此输入
gallery-tf-caption-fluent-text = 有文字 · 出现清除按钮
gallery-tf-caption-fluent-disabled = 禁用
gallery-tf-caption-filled-label = filled · 标签与辅助文本
gallery-tf-caption-filled-floats = filled · 文字浮起标签
gallery-tf-caption-outlined-floats = outlined · 文字浮起标签
gallery-tf-caption-outlined-icon = outlined · 前置图标
gallery-tf-caption-outlined-error = outlined · 错误
gallery-tf-caption-outlined-disabled = outlined · 禁用
gallery-tf-caption-glass = 玻璃 · 前置图标 · 在此输入
gallery-tf-caption-glass-search = 玻璃 · 搜索变体，高 44
gallery-tf-caption-glass-text = 玻璃 · 禁用，带文字
gallery-tf-caption-glass-secure = glass · 密码：只画掩码，不可复制
gallery-tf-secure-placeholder = 密码
gallery-tf-clear-button = 清空
gallery-search-hint = 搜索
gallery-search-hint-contained = 容纳式
gallery-search-caption-docked = 嵌入式 · 前置图标 · 一个动作
gallery-search-caption-contained = 容纳式
gallery-search-caption-long = 一条比栏更宽的查询
gallery-search-supporting = 搜索设置、应用和文件
gallery-search-caption-supporting = 辅助文本与尾部头像
gallery-search-caption-glass = 玻璃 · 清除 × 与取消按钮
gallery-search-caption-glass-quiet = 玻璃 · 无取消按钮
gallery-search-view-hint = 搜索本机
gallery-search-caption-view = 搜索视图 · 点栏展开，点返回箭头收起
gallery-search-readout = 应用查询：{ $query } · 上次动作：{ $action }

## 文本选区页。
## Text selection.

gallery-selection-prose = 在这段文字上拖动即可选中。高亮的几何归引擎，如何绘制归风格包。双击一个词只取那个词，并观察三列的差别。

## 包驱动的通用控件页。
## Generic pack-driven widgets.

gallery-generic-button = 包按钮
gallery-generic-card-title = 卡片
gallery-generic-column-card = 卡片 + 按钮
gallery-generic-column-list = 列表
gallery-generic-column-panel = 嵌板
gallery-generic-column-paged = 分页条带
gallery-generic-page = 第 { $index } 页
gallery-generic-row = 第 { $index } 行
gallery-generic-panel-title = 嵌板
gallery-generic-panel-hint = 由包驱动
gallery-generic-column-collapse = 可折叠行
gallery-generic-banner-text = 可关闭的横幅
gallery-generic-banner-hide = 隐藏横幅
gallery-generic-banner-show = 显示横幅
gallery-generic-column-grow = 高度动画卡片
gallery-generic-note = 一行会来会走的文字。
gallery-generic-note-add = 加一行
gallery-generic-note-remove = 去掉这一行

## 网格页。
## Layout grids.

gallery-grid-column-md3 = MD3 · 固定 + fr
gallery-grid-column-fluent = Fluent · auto 列
gallery-grid-column-glass = Glass · 跨度 + 对齐
gallery-grid-column-dense = Dense · repeat(auto-fill) + minmax
gallery-grid-tile-row2 = 第 2 行
gallery-grid-tile-span = 跨两列
gallery-grid-tile-pinned = 钉住
gallery-grid-tile-two-span = 两格
gallery-grid-tile-long-label = 一个更宽的标签
gallery-grid-tile-second = 第二个
gallery-grid-tile-row = 行
gallery-grid-tile-here = 这里
gallery-grid-tile-one = 一
gallery-grid-tile-mid = 中
gallery-grid-tile-two-rows = 跨两行
gallery-grid-tile-two-columns = 第 1 行 · 两列
gallery-grid-tile-centred = 居中

## 行内页。
## Inline.

gallery-inline-column-baseline = 基线 · 两个图标
gallery-inline-column-mixed = 顶部 · 居中 · 底部
gallery-inline-column-narrow = 窄块 · 原子自起一行
gallery-inline-column-row = 跨项基线
gallery-inline-column-decorations = 装饰 · 单线、双线、点线、波浪线
gallery-inline-column-justify = 两端对齐 · 左右边缘齐平
gallery-inline-column-holder = 行内盒 · 句子中的一个控件
gallery-inline-sync-1 = 正在同步{" "}
gallery-inline-sync-2 = {" "}到服务器，另有{" "}
gallery-inline-sync-3 = {" "}三个文件
gallery-inline-tall-1 = 高{" "}
gallery-inline-tall-2 = {" "}顶对齐，{" "}
gallery-inline-tall-3 = {" "}居中，{" "}
gallery-inline-tall-4 = {" "}底部对齐。
gallery-inline-narrow-1 = 之前{" "}
gallery-inline-narrow-2 = {" "}之后

## 定位页。
## Positioning.

gallery-positioning-column-relative = 相对 · 保留原位置
gallery-positioning-column-absolute = 绝对 · 内边距盒 + 自动外边距
gallery-positioning-column-fixed = 固定 · 根盒子，以及 z 顺序
gallery-positioning-tile-shifted = 已位移
gallery-positioning-tile-pin = 贴角
gallery-positioning-tile-auto = 自动
gallery-positioning-tile-in-flow = 流内
gallery-positioning-tile-fixed = 固定

## 骨架区域。
## Scaffold regions.

gallery-scaffold-nav = 导航
gallery-scaffold-header = 页眉
gallery-scaffold-main = 主体
gallery-scaffold-context = 上下文
gallery-scaffold-inline = 行内
gallery-scaffold-overlay = 浮层

## 材质页。
## Materials.

gallery-materials-plain = 素面
gallery-materials-acrylic = 亚克力
gallery-materials-glass = 玻璃

## 设计体系页。
## Design-system pages.

gallery-system-colours = { $system } · 颜色
gallery-system-dimensions = { $system } · 尺寸
gallery-system-provenance = 来源
gallery-system-common-roles = 公共角色
gallery-system-native-extras = 原生额外项

## 进度、导航、工具条与提示条页。
## Progress, navigation, toolbar and snackbar pages.

gallery-icon-button-styles = 四种样式 · 标准、实心、色调、描边
gallery-icon-button-sizes = 五种尺寸 · 32、40、56、96 与 136 dp
gallery-icon-button-widths = medium 下的三种宽度 · 窄 48、默认 56、宽 72
gallery-icon-button-toggles = 点击切换选中 · 选中的圆角是 12 dp
gallery-icon-button-toggles-title = MD3 · 开关
gallery-icon-button-readout = 已选中：{ $on }
gallery-icon-button-none = 尚无
gallery-icon-button-disabled = 禁用 · 令牌自带的 38% 图标与 10% 填充
gallery-progress-caption = 轨道的 { $percent }%
gallery-progress-note = 圆环轨道在弧的两端各让出 4dp；这个空隙就是导出的 track_active_indicator_space，线性条用的也是同一个令牌。
gallery-progress-error = 轨道的 60%，错误色
gallery-progress-paused = 轨道的 60%，暂停
gallery-progress-indeterminate = 不确定态 —— 两条指示条，两秒一个周期
gallery-progress-md3-indeterminate = 不确定态 —— 两条指示条，按 Android 的 1.8 秒扫动
gallery-progress-wavy = 60% 轨道的波浪形态 —— 表现力样式，10dp 高
gallery-progress-wavy-indeterminate = 不确定态 —— 两道波浪，按 1.8 秒时钟扫动
gallery-progress-md3-ring-indeterminate = 不确定态 —— 40dp 圆环，弧长大并行进
gallery-progress-fluent-note = 3dp 槽、1dp 轨道、圆角指示条；扫动自己读取帧时钟。
gallery-progress-fluent-rings = 进度环：0、25、60、100%，然后是旋转中与未激活 —— WinUI 3 的轨道画刷是透明的
gallery-progress-glass-rings = 20dp 的圆环：0、25、60、100%，然后是一个 35% 的 40dp 圆环与一个扫动中的
gallery-progress-glass-bars = 横条：0、25、60、100%，然后是一条扫动中的 —— 高 4dp，最窄 200dp
gallery-progress-glass-note = 扫动按自己的一秒时钟走；辉光是更宽的半透明描边，因为引擎不模糊弧。

gallery-segmented-single = 单选 —— 点击替换选择
gallery-segmented-multi = 多选 —— 点击翻转该段
gallery-segmented-disabled = 禁用 —— 内容 38%、描边 12%
gallery-segmented-day = 日
gallery-segmented-week = 周
gallery-segmented-month = 月
gallery-segmented-readout = 单选 { $single }，多选 { $multi }
gallery-segmented-note = 高 40dp；每段上报自己的 id，选中集合归应用所有。
gallery-segmented-glass = 文字 —— 13pt，未选中 w500 60%，选中 w600
gallery-segmented-glass-icons = 图标 —— 等宽格子里 20pt 字形
gallery-segmented-glass-readout = 玻璃选中 { $index }

gallery-nav-caption = 标签：{ $kind ->
  [all] 全部显示
  [selected] 仅选中项
  [expressive] 在胶囊内
 *[none] 不显示
}
gallery-nav-home = 首页
gallery-nav-search = 搜索
gallery-nav-profile = 个人
gallery-nav-bar-note = 80dp 的栏；胶囊在 500ms 内沿 x 轴展开。
gallery-nav-rail-note = 80dp 的列；不显示标签时胶囊变成 56×56 的正圆。
gallery-nav-rail-expand = 展开 / 收起
gallery-nav-rail-expressive-note = expressive：96dp 的列与 220dp 的面板是同一个控件 —— 300ms standard 把宽度带过去，胶囊随之张开。

gallery-drawer-headline = 邮件
gallery-drawer-caption = 标准 —— 一个分组标题下挂三个目标
gallery-drawer-inbox = 收件箱
gallery-drawer-sent = 已发送
gallery-drawer-drafts = 草稿
gallery-drawer-note = 360dp 表面、内缩 12dp 的 336×56 胶囊；标准变体，无遮罩。
gallery-drawer-open = 打开抽屉
gallery-drawer-modal-caption = modal —— 页面在 32% 遮罩后变暗，按下抽屉外侧请求关闭

gallery-toolbar-caption = { $kind ->
  [docked] 停靠式
 *[floating] 浮动式
} · { $theme ->
  [standard] 标准
 *[vibrant] 鲜艳
}
gallery-toolbar-note = 64dp 的条上装 48dp 图标按钮槽。

gallery-snackbar-message = 消息已归档。
gallery-snackbar-two-lines = 这条消息需要第二行才能说清到底发生了什么。
gallery-snackbar-action = 撤销
gallery-snackbar-one-line = 单行带动作
gallery-snackbar-two-line-caption = 两行，即 68dp 分支
gallery-snackbar-floating = 浮动式，正文内缩 16dp
gallery-snackbar-hidden = 隐藏 —— 什么都不画
gallery-snackbar-hits = 动作被点击 { $count } 次
gallery-snackbar-note = 应用负责它的生命周期；条淡入淡出。
gallery-snackbar-overlay-show = 显示浮动条
gallery-snackbar-overlay-caption = 浮动式 —— 浮在页面底部，不占流

## Info bar.
## 信息栏页。

gallery-infobar-caption = WinUI 信息栏 —— 按严重级别着色的 48dp 条带、填色的状态圆盘、38dp 关闭按钮
gallery-infobar-info-title = 有可用更新。
gallery-infobar-info-message = 重启应用即可安装最新更新。
gallery-infobar-success-title = 备份完成。
gallery-infobar-success-message = 所有文档均已成功上传。
gallery-infobar-warning-title = 你的订阅将在 3 天后到期。
gallery-infobar-warning-message = 续订订阅以保留全部功能。
gallery-infobar-error-title = 无网络连接
gallery-infobar-error-message = 重新连接后继续工作。
gallery-infobar-reopen = 重新显示已关闭的条
gallery-infobar-narrow-caption = 窄宽度 —— 标题与消息无法共处一行时，面板改为堆叠
gallery-infobar-narrow-title = 备份完成
gallery-infobar-narrow-message = 所有文档均已成功上传；这条信息栏足够窄，消息已经无法与标题并排，因此两者堆叠。

gallery-markdown-sample =
    # 排好版面的 Markdown

    一个段落里可以有好几种 run：*强调*、**加粗**、`行内代码`、~~删除线~~ 和一个 [链接](https://example.com)，整段按同一个宽度换行，标记跟着词走。

    - 列表项装的是块，不是字符串
    - 嵌套也可以：

      1. 有序列表从它自己的起始编号开始
      2. 每一项保留自己的段落


    > 引用和别的容器一样，里面装块。

    - [x] 任务项画的是方框，不是它的标记
    - [ ] 而标记的字节仍然留在源里

    ```rust
    let runs = to_text_block(&document, block, base, look);
    let first = runs.iter().position(|run| run.visibility.painted());
    let longest = runs.iter().map(|run| run.advance).fold(0.0, f32::max);
    let digits = line_count.to_string().len();
    let gutter = advance(&"0".repeat(digits), style) + CODE_NUMBER_GAP;
    // A comment that runs past the card's far edge, so the wrap has something to break
    let inner = width - gutter;
    let rows = wrap(runs, inner, gutter, gutter);
    let numbers = rows.iter().enumerate().map(|(index, row)| index + 1);
    let painted = paint(numbers, rows, gutter);
    ```

    ---

    行内公式如 $a^2 + b^2 = c^2$ 是一个 mark；行间公式是一个块：

    $$
    \int_0^1 x^2 \, dx
    $$

## 公式页：每套设计系统给公式的尺寸与墨色。

gallery-math-display-caption = 行间 — { $size }, { $ink }
gallery-math-inline-caption = 行内 — { $size }, { $ink }
gallery-math-refused-caption = 被拒绝的源码 — 悬停查看被框住的部分
gallery-math-structured-caption = 结构化 — 点击槽位后直接输入
