-- The look the game's panels and menus share.

local ui = {}

ui.BORDER = "#f2d3ab"
ui.PANEL = "#14182c"
ui.TEXT = "#f4f0e8"
-- Speakers' names, and the pointer at the highlighted choice.
ui.HIGHLIGHT = "#f2c14e"
-- Labels, and choices that aren't highlighted.
ui.DIM = "#8a92b8"

-- The frame panels are drawn in, `frame.png`: a border a pixel wide in the border's color, its
-- corners cut, round the panel's color. Its edges, `FRAME_SLICE` texels in, keep their size
-- however big a panel is. Panels laid out in scenes, as the title's menu and the bag's stats
-- are, show it too.
ui.FRAME = graphics.load_sprite_sheet("frame.png", 5, 5):sprite(0, 0)
ui.FRAME_SLICE = 2

-- Draws a panel with a border, `width` by `height` with its top-left corner at `x`, `y`.
function ui.panel(x: number, y: number, width: number, height: number)
    graphics.draw_sprite_sliced(ui.FRAME, x, y, width, height, ui.FRAME_SLICE)
end

return ui
