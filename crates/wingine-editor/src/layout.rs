//! Laying out screens: anchor presets, as Godot's, which place a node at a point of its parent's
//! rectangle or stretch it across, as scene files say it: an `anchor`, a `pivot` at the same point
//! of the node, and `stretch`.

use wingine::{
    graphics::Align,
    scene::{KindDef, NodeDef},
};

use Place::{At, Wide};

/// Where a preset puts a node along one axis of its parent's rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    /// With the same fraction of the node at that fraction of the parent: 0 at its start, 1 at
    /// its end.
    At(f32),
    /// Stretched from the parent's start to its end.
    Wide,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    /// Across, then down.
    pub places: [Place; 2],
}

const fn preset(name: &'static str, x: Place, y: Place) -> Preset {
    Preset {
        name,
        places: [x, y],
    }
}

/// The presets, in rows of four as Godot lays them out: the corners and edges' middles, then the
/// ways to stretch across.
pub const PRESETS: [Preset; 16] = [
    preset("Top Left", At(0.0), At(0.0)),
    preset("Center Top", At(0.5), At(0.0)),
    preset("Top Right", At(1.0), At(0.0)),
    preset("Top Wide", Wide, At(0.0)),
    preset("Center Left", At(0.0), At(0.5)),
    preset("Center", At(0.5), At(0.5)),
    preset("Center Right", At(1.0), At(0.5)),
    preset("HCenter Wide", Wide, At(0.5)),
    preset("Bottom Left", At(0.0), At(1.0)),
    preset("Center Bottom", At(0.5), At(1.0)),
    preset("Bottom Right", At(1.0), At(1.0)),
    preset("Bottom Wide", Wide, At(1.0)),
    preset("Left Wide", At(0.0), Wide),
    preset("VCenter Wide", At(0.5), Wide),
    preset("Right Wide", At(1.0), Wide),
    preset("Full Rect", Wide, Wide),
];

/// Whether `node` can take `preset`: tile layers sit where their maps are, nodes placed in 3D
/// have no anchors, and only nodes with a `stretch` can stretch.
pub fn fits(node: &NodeDef, preset: &Preset) -> bool {
    let kind = &node.kind;
    let stretches = matches!(
        kind,
        KindDef::Sprite(_)
            | KindDef::Rect(_)
            | KindDef::Area(_)
            | KindDef::Stack(_)
            | KindDef::Panel(_)
            | KindDef::View3d(_)
    );
    let wide = preset.places.contains(&Wide);
    let placed = matches!(kind, KindDef::TileLayer(_)) || node.placement3.is_some();
    !placed && (stretches || !wide)
}

/// Lays `node` out by `preset`, `margin` in from its parent's edges, keeping its size, which is
/// `size` as drawn. Its `x` and `y` become the margin's.
pub fn apply(node: &mut NodeDef, preset: &Preset, size: [f32; 2], margin: f32) {
    let mut anchor = [0.0; 2];
    let mut offset = [0.0; 2];
    for (i, place) in preset.places.iter().enumerate() {
        match *place {
            At(fraction) => {
                anchor[i] = fraction;
                // In from whichever edge it's at, and not at all in the middle.
                offset[i] = margin * (1.0 - 2.0 * fraction);
            }
            Wide => offset[i] = margin,
        }
    }
    node.anchor = Some(anchor);
    [node.x, node.y] = offset;
    let (own, stretch, pivot) = match &mut node.kind {
        KindDef::Sprite(sprite) => {
            // Its own size stays its image's unless it has to change.
            let natural = sprite.size.is_none() && sprite.stretch == [0.0, 0.0];
            let mut own = sprite.size.unwrap_or(size);
            let mut stretch = sprite.stretch;
            lay_out(
                preset,
                size,
                margin,
                &mut own,
                &mut stretch,
                &mut sprite.pivot,
            );
            sprite.stretch = stretch;
            sprite.size = (!natural || stretch != [0.0, 0.0] || own != size).then_some(own);
            return;
        }
        KindDef::Rect(rect) => (&mut rect.size, &mut rect.stretch, &mut rect.pivot),
        KindDef::Area(area) => (&mut area.size, &mut area.stretch, &mut area.pivot),
        KindDef::Stack(_) | KindDef::Panel(_) => {
            // A stack's anchor lines it up by itself, so its pivot is from there, and one that
            // fits what it holds keeps fitting it, unless it stretches across its parent.
            let Some(stack) = node.kind.stack_mut() else {
                return;
            };
            for (i, place) in preset.places.iter().enumerate() {
                stack.pivot[i] = 0.0;
                match *place {
                    At(_) => {
                        if stack.size[i].is_some() {
                            stack.size[i] = Some(size[i]);
                        }
                        stack.stretch[i] = 0.0;
                    }
                    Wide => {
                        stack.size[i] = Some(-2.0 * margin);
                        stack.stretch[i] = 1.0;
                    }
                }
            }
            return;
        }
        KindDef::View3d(view) => (&mut view.size, &mut view.stretch, &mut view.pivot),
        KindDef::Text(text) => {
            // Text lines up with the edge or middle it's placed at.
            let At(across) = preset.places[0] else {
                return;
            };
            text.align = if across < 0.25 {
                Align::Left
            } else if across > 0.75 {
                Align::Right
            } else {
                Align::Center
            };
            let down = match preset.places[1] {
                At(down) => down,
                Wide => 0.0,
            };
            text.pivot = [0.0, down * size[1]];
            return;
        }
        KindDef::Node
        | KindDef::Tilemap(_)
        | KindDef::TileLayer(_)
        | KindDef::Instance(_)
        | KindDef::Mesh(_)
        | KindDef::Model(_)
        | KindDef::Billboard(_) => {
            return;
        }
    };
    lay_out(preset, size, margin, own, stretch, pivot);
}

/// The size, stretch and pivot of a node `size` in size as drawn, laid out by `preset`.
fn lay_out(
    preset: &Preset,
    size: [f32; 2],
    margin: f32,
    own: &mut [f32; 2],
    stretch: &mut [f32; 2],
    pivot: &mut [f32; 2],
) {
    for (i, place) in preset.places.iter().enumerate() {
        match *place {
            At(fraction) => {
                own[i] = size[i];
                stretch[i] = 0.0;
                pivot[i] = fraction * size[i];
            }
            Wide => {
                own[i] = -2.0 * margin;
                stretch[i] = 1.0;
                pivot[i] = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use wingine::{
        graphics::Color,
        scene::{RectDef, SpriteDef, Stack, TextDef},
    };

    use super::*;

    fn named(name: &str) -> &'static Preset {
        PRESETS.iter().find(|preset| preset.name == name).unwrap()
    }

    fn rect() -> NodeDef {
        let mut node = NodeDef::new(
            "bar",
            KindDef::Rect(RectDef {
                size: [40.0, 10.0],
                stretch: [0.0, 0.0],
                pivot: [0.0, 0.0],
                color: Color::WHITE,
            }),
        );
        node.x = 7.0;
        node
    }

    #[test]
    fn corners_put_the_node_flush_in_them() {
        let mut node = rect();
        apply(&mut node, named("Bottom Right"), [40.0, 10.0], 0.0);
        assert_eq!(node.anchor, Some([1.0, 1.0]));
        assert_eq!([node.x, node.y], [0.0, 0.0]);
        let KindDef::Rect(def) = &node.kind else {
            unreachable!()
        };
        assert_eq!(def.pivot, [40.0, 10.0]);

        // In by the margin.
        apply(&mut node, named("Center Left"), [40.0, 10.0], 4.0);
        assert_eq!([node.x, node.y], [4.0, 0.0]);
        let KindDef::Rect(def) = &node.kind else {
            unreachable!()
        };
        assert_eq!(def.pivot, [0.0, 5.0]);
    }

    #[test]
    fn wide_presets_stretch_with_the_parent() {
        let mut node = rect();
        apply(&mut node, named("Bottom Wide"), [40.0, 10.0], 8.0);
        assert_eq!(node.anchor, Some([0.0, 1.0]));
        assert_eq!([node.x, node.y], [8.0, -8.0]);
        let KindDef::Rect(def) = &node.kind else {
            unreachable!()
        };
        // 16 narrower than its parent, and as tall as it was.
        assert_eq!(def.size, [-16.0, 10.0]);
        assert_eq!(def.stretch, [1.0, 0.0]);
        assert_eq!(def.pivot, [0.0, 10.0]);

        // Back to a corner, at the size it was drawn.
        apply(&mut node, named("Top Left"), [300.0, 10.0], 0.0);
        let KindDef::Rect(def) = &node.kind else {
            unreachable!()
        };
        assert_eq!(def.size, [300.0, 10.0]);
        assert_eq!(def.stretch, [0.0, 0.0]);
    }

    #[test]
    fn sprites_keep_their_own_size_until_they_stretch() {
        let mut node = NodeDef::new(
            "icon",
            KindDef::Sprite(SpriteDef {
                sheet: "tiles".into(),
                cell: [0, 0],
                size: None,
                stretch: [0.0, 0.0],
                pivot: [0.0, 0.0],
                tint: Color::WHITE,
            }),
        );
        apply(&mut node, named("Center"), [16.0, 16.0], 0.0);
        let KindDef::Sprite(def) = &node.kind else {
            unreachable!()
        };
        assert_eq!((def.size, def.pivot), (None, [8.0, 8.0]));
        apply(&mut node, named("Full Rect"), [16.0, 16.0], 0.0);
        let KindDef::Sprite(def) = &node.kind else {
            unreachable!()
        };
        assert_eq!((def.size, def.stretch), (Some([0.0, 0.0]), [1.0, 1.0]));
    }

    #[test]
    fn stacks_line_up_by_their_anchors_and_keep_fitting() {
        let mut node = NodeDef::new("menu", KindDef::Stack(Stack::FITTING));
        apply(&mut node, named("Bottom Right"), [60.0, 40.0], 4.0);
        assert_eq!(node.anchor, Some([1.0, 1.0]));
        assert_eq!([node.x, node.y], [-4.0, -4.0]);
        assert_eq!(
            node.kind.stack(),
            Some(&Stack::FITTING),
            "no pivot, still fitting"
        );

        apply(&mut node, named("Top Wide"), [60.0, 40.0], 4.0);
        let stack = node.kind.stack().unwrap();
        assert_eq!(stack.size, [Some(-8.0), None]);
        assert_eq!(stack.stretch, [1.0, 0.0]);

        // Back to a corner, at the size it was drawn across, and still fitting down.
        apply(&mut node, named("Center"), [300.0, 40.0], 0.0);
        let stack = node.kind.stack().unwrap();
        assert_eq!(
            (stack.size, stack.stretch),
            ([Some(300.0), None], [0.0, 0.0])
        );
    }

    #[test]
    fn text_aligns_with_where_it_is_placed() {
        let mut node = NodeDef::new(
            "score",
            KindDef::Text(TextDef {
                text: "000".into(),
                font_scale: 1.0,
                wrap: None,
                align: Align::Left,
                pivot: [0.0, 0.0],
                color: Color::WHITE,
            }),
        );
        assert!(fits(&node, named("Center Bottom")));
        assert!(!fits(&node, named("Top Wide")));
        apply(&mut node, named("Center Bottom"), [30.0, 8.0], 2.0);
        let KindDef::Text(def) = &node.kind else {
            unreachable!()
        };
        assert_eq!((def.align, def.pivot), (Align::Center, [0.0, 8.0]));
        assert_eq!([node.x, node.y], [0.0, -2.0]);
    }
}
