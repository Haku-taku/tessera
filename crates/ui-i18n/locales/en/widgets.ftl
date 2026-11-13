# Strings owned by ui-widgets: shortcut labels and list counts.

## Shortcuts

widgets-shortcut-ctrl = Ctrl
widgets-shortcut-alt = Alt
widgets-shortcut-shift = Shift
widgets-shortcut-super = Super
# Separator between held modifiers and the key; spaces belong to the translation.
widgets-shortcut-separator = { " + " }
widgets-shortcut-escape = Esc
widgets-shortcut-enter = Enter
widgets-shortcut-space = Space
widgets-shortcut-tab = Tab
widgets-shortcut-backspace = Backspace
widgets-shortcut-delete = Delete
widgets-shortcut-home = Home
widgets-shortcut-end = End
widgets-shortcut-left = Left
widgets-shortcut-right = Right
widgets-shortcut-up = Up
widgets-shortcut-down = Down

## Lists

# CLDR plural categories are variant keys, not a "-plural" suffix.
widgets-list-selected-count =
    { $count ->
        [one] { $count } item selected
       *[other] { $count } items selected
    }

## Code cards

# A fence with no info string still has a name in the card's head.
widgets-code-plain-text = Plain text
widgets-code-line-count =
    { $count ->
        [one] { $count } line
       *[other] { $count } lines
    }

## Menus

# The engine's built-in text rows; an app names them through `set_text_menu_labels`.
widgets-menu-select-all = Select all
widgets-menu-copy = Copy
widgets-menu-cut = Cut
widgets-menu-paste = Paste
