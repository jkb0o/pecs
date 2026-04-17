//! This example shows how to use `pecs` for organizing UI logic
//! with async operations. We create an exit button that exits the app.
use bevy::prelude::*;
use pecs::prelude::*;

const COLOR_DARK: Color = Color::srgb(0.2, 0.2, 0.2);
const COLOR_LIGHT: Color = Color::srgb(0.8, 0.8, 0.8);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PecsPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn((Node {
        display: Display::Flex,
        width: Val::Percent(100.),
        height: Val::Percent(100.),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    },));

    let exit_button = commands
        .spawn((
            Button,
            Node {
                width: Val::Px(150.),
                height: Val::Px(65.),
                margin: UiRect::all(Val::Auto),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(COLOR_DARK),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Exit"),
                TextFont::from_font_size(40.0),
                TextColor(COLOR_LIGHT),
            ));
        })
        .id();

    commands.queue(
        Promise::new(
            exit_button,
            asyn!(state => {
                asyn::ui::button(state.value).pressed()
            }),
        )
        .then(asyn!(state, _ => {
            info!("Exit button pressed!");
            asyn::app::exit()
        })),
    );
}
