# Strings owned by ui-picker: the file dialog's chrome.
# The picker ships its own messages rather than borrowing the opening app's.

## Columns and states

picker-name = Name
picker-size = Size
picker-modified = Modified
picker-loading = Loading…
picker-searching = Searching…
picker-empty = This folder is empty
picker-no-matches = No matches
picker-failed = This folder could not be read

## Controls

picker-filter-all-files = All files
picker-up = Parent folder
picker-show-hidden = Show hidden files
picker-open = Open
picker-save = Save
picker-save-files = Saving { $count } files
picker-select-folder = Select folder
picker-select-folders = Select folders
picker-cancel = Cancel
picker-retry = Retry
picker-path-failed = That path could not be opened
picker-overwrite = “{ $name }” already exists. Replace it?
picker-replace = Replace

## Drag and drop

picker-move-here = Move Here
picker-copy-here = Copy Here
picker-link-here = Create Link Here

## Places

picker-place-home = Home
picker-place-desktop = Desktop
picker-place-documents = Documents
picker-place-downloads = Downloads
picker-place-music = Music
picker-place-pictures = Pictures
picker-place-videos = Videos
picker-place-templates = Templates
picker-place-public = Public

## Devices

picker-devices = Devices
picker-device-root = Root
picker-removable = Removable
picker-removable-unnamed = Unnamed volume
picker-mount-failed = The device could not be mounted

## Demo harness

picker-demo-title = File picker
picker-demo-hint = Open the picker in a pack to try it.
picker-demo-chosen = Open: { $path }
picker-demo-cancelled = Cancelled
picker-demo-failed = Failed: { $reason }
picker-demo-no-surface = This host cannot open a dialog window; the picker opens in this one.

## File verbs

picker-cut = Cut
picker-copy = Copy
picker-paste = Paste
picker-paste-items = Paste { $count } items
picker-move-to = Move to…
picker-copy-to = Copy to…
picker-rename = Rename
picker-trash = Move to Trash
picker-delete = Delete Permanently
picker-copy-path = Copy Path
picker-new-folder = New Folder
picker-new-folder-name = New folder
picker-select-all = Select All
picker-invert-selection = Invert Selection
picker-refresh = Refresh
picker-create-link = Create Link
picker-create-hard-link = Create Hard Link
picker-more-options = More Options…
picker-shell-menu = Shell Menu
picker-shell-partial = Some items are only in the system menu
picker-link-unsupported = This platform cannot create a hard link
picker-symlink-unsupported = This platform cannot create a link
picker-link-across-volumes = A hard link must stay on one volume
picker-link-files-only = Only files can be hard-linked
picker-undo = Undo
picker-undo-move = Undo Move
picker-undo-rename = Undo Rename
picker-undo-copy = Undo Copy

## Rename

picker-rename-invalid = That name cannot be used
picker-rename-taken = “{ $name }” already exists

## Properties

picker-properties = Properties
picker-properties-items =
    { $count ->
        [one] 1 item
       *[other] { $count } items
    }
picker-properties-type = Type
picker-properties-type-folder = Folder
picker-properties-type-file = File
picker-properties-type-extension = { $ext } file
picker-properties-location = Location
picker-properties-size = Size
picker-properties-modified = Modified

## File operations

picker-copying = Copying { $done } of { $total } — { $percent }%
picker-moving = Moving { $done } of { $total } — { $percent }%
picker-linking = Creating links { $done } of { $total } — { $percent }%
picker-deleting = Deleting { $done } of { $total }
picker-renaming = Renaming…
picker-operation-failed =
    { $count ->
        [one] One item could not be completed
       *[other] { $count } items could not be completed
    }

## Conflicts

picker-conflict = “{ $name }” already exists in this folder
picker-conflict-replace = Replace
picker-conflict-skip = Skip
picker-conflict-keep-both = Keep Both
picker-conflict-rename = Rename
picker-conflict-apply-all = Apply to all
picker-dismiss = Close
picker-delete-confirm =
    { $count ->
        [one] Delete this item permanently? This cannot be undone.
       *[other] Delete { $count } items permanently? This cannot be undone.
    }
picker-undo-copy-confirm =
    { $count ->
        [one] Undo the copy? The item it created will be removed.
       *[other] Undo the copy? The { $count } items it created will be removed.
    }

## Operation errors

picker-error-not-found = The file no longer exists
picker-error-denied = Permission denied
picker-error-read-only = The file is read-only
picker-error-exists = The name is already taken
picker-error-already-there = It is already in that folder
picker-error-cross-device = The volume refused the move
picker-error-recursive = A folder cannot contain itself
picker-error-unsupported = This platform does not support that
picker-error-other = The operation failed
picker-failure-item = { $name } — { $reason }
