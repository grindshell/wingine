//! Embeds the game's folder in the module: the one `WINGINE_GAME` names, or the RPG example.

fn main() {
    wingine_web::embed_game("../../examples/rpg");
}
