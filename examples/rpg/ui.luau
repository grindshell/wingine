-- The look the game's panels and menus share.

local ui = {}

ui.BORDER = "#f2d3ab"
ui.PANEL = "#14182c"
ui.TEXT = "#f4f0e8"
-- Speakers' names, and the pointer at the highlighted choice.
ui.HIGHLIGHT = "#f2c14e"
-- Labels, and choices that aren't highlighted.
ui.DIM = "#8a92b8"

-- Draws a panel with a border, `width` by `height` with its top-left corner at `x`, `y`.
function ui.panel(x: number, y: number, width: number, height: number)
    graphics.fill_rect(x, y, width, height, ui.BORDER)
    graphics.fill_rect(x + 1, y + 1, width - 2, height - 2, ui.PANEL)
end

return ui
