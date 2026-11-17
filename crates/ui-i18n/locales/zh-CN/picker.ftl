# ui-picker 所有的文案：文件对话框的界面文字。
# 选择器自带消息，不使用打开它的应用的目录。

## 列与状态

picker-name = 名称
picker-size = 大小
picker-modified = 修改时间
picker-loading = 正在载入…
picker-searching = 正在搜索…
picker-empty = 此文件夹为空
picker-no-matches = 没有匹配项
picker-failed = 无法读取此文件夹

## 控件

picker-filter-all-files = 所有文件
picker-up = 上级文件夹
picker-show-hidden = 显示隐藏文件
picker-open = 打开
picker-save = 保存
picker-save-files = 正在保存 { $count } 个文件
picker-select-folder = 选择此文件夹
picker-select-folders = 选择这些文件夹
picker-cancel = 取消
picker-retry = 重试
picker-path-failed = 无法打开该路径
picker-overwrite = “{ $name }” 已存在，要替换吗？
picker-replace = 替换

## 拖放

picker-move-here = 移动到此处
picker-copy-here = 复制到此处
picker-link-here = 在此创建链接

## 位置

picker-place-home = 主目录
picker-place-desktop = 桌面
picker-place-documents = 文档
picker-place-downloads = 下载
picker-place-music = 音乐
picker-place-pictures = 图片
picker-place-videos = 视频
picker-place-templates = 模板
picker-place-public = 公共

## 设备

picker-devices = 设备
picker-device-root = 根目录
picker-removable = 可移动设备
picker-removable-unnamed = 未命名卷
picker-mount-failed = 无法挂载该设备

## 演示

picker-demo-title = 文件选择器
picker-demo-hint = 用任意一种样式打开选择器试试。
picker-demo-chosen = 打开：{ $path }
picker-demo-cancelled = 已取消
picker-demo-failed = 失败：{ $reason }
picker-demo-no-surface = 此主机无法打开对话框窗口，选择器改为在当前窗口中打开。

## 文件操作动词

picker-cut = 剪切
picker-copy = 复制
picker-paste = 粘贴
picker-paste-items = 粘贴 { $count } 项
picker-move-to = 移动到…
picker-copy-to = 复制到…
picker-rename = 重命名
picker-trash = 移到回收站
picker-delete = 永久删除
picker-copy-path = 复制路径
picker-new-folder = 新建文件夹
picker-new-folder-name = 新建文件夹
picker-select-all = 全选
picker-invert-selection = 反向选择
picker-refresh = 刷新
picker-create-link = 创建链接
picker-create-hard-link = 创建硬链接
picker-more-options = 更多选项…
picker-shell-menu = 系统菜单
picker-shell-partial = 部分项目仅在系统菜单中显示
picker-link-unsupported = 此平台无法创建硬链接
picker-symlink-unsupported = 此平台无法创建链接
picker-link-across-volumes = 硬链接必须位于同一卷
picker-link-files-only = 只有文件才能创建硬链接
picker-undo = 撤销
picker-undo-move = 撤销移动
picker-undo-rename = 撤销重命名
picker-undo-copy = 撤销复制

## 重命名

picker-rename-invalid = 该名称不可用
picker-rename-taken = “{ $name }” 已存在

## 属性

picker-properties = 属性
picker-properties-items =
    { $count ->
       *[other] { $count } 个项目
    }
picker-properties-type = 类型
picker-properties-type-folder = 文件夹
picker-properties-type-file = 文件
picker-properties-type-extension = { $ext } 文件
picker-properties-location = 位置
picker-properties-size = 大小
picker-properties-modified = 修改时间

## 文件操作

picker-copying = 正在复制第 { $done } 项，共 { $total } 项 — { $percent }%
picker-moving = 正在移动第 { $done } 项，共 { $total } 项 — { $percent }%
picker-linking = 正在创建链接，第 { $done } 项，共 { $total } 项 — { $percent }%
picker-deleting = 正在删除第 { $done } 项，共 { $total } 项
picker-renaming = 正在重命名…
picker-operation-failed =
    { $count ->
        [one] 有 1 项无法完成
       *[other] 有 { $count } 项无法完成
    }

## 冲突

picker-conflict = “{ $name }” 已存在于此文件夹
picker-conflict-replace = 替换
picker-conflict-skip = 跳过
picker-conflict-keep-both = 保留两者
picker-conflict-rename = 重命名
picker-conflict-apply-all = 全部应用
picker-dismiss = 关闭
picker-delete-confirm =
    { $count ->
        [one] 要永久删除这一项吗？此操作无法撤销。
       *[other] 要永久删除这 { $count } 项吗？此操作无法撤销。
    }
picker-undo-copy-confirm =
    { $count ->
        [one] 要撤销复制吗？它创建的这一项将被移除。
       *[other] 要撤销复制吗？它创建的这 { $count } 项将被移除。
    }

## 操作错误

picker-error-not-found = 文件已不存在
picker-error-denied = 权限不足
picker-error-read-only = 文件是只读的
picker-error-exists = 名称已被占用
picker-error-already-there = 该项已在此文件夹中
picker-error-cross-device = 该卷拒绝了移动
picker-error-recursive = 文件夹不能包含自身
picker-error-unsupported = 此平台不支持该操作
picker-error-other = 操作失败
picker-failure-item = { $name } — { $reason }
