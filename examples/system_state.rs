//! This example shows how promises keep the state of Bevy's system params.
//! We create 16 buttons and asyn loop single promise every second.
//! Inside the promise we log buttons with changed for the previous second
//! `Interaction` component by querying with Changed<Interaction> filter.
use bevy::prelude::*;
use pecs::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PecsPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.promise(|| ()).then_repeat(asyn!(
        buttons: Query<&Name, Changed<Interaction>>
    => {
        if buttons.is_empty() {
            info!("No changes");
        } else {
            info!("Changed buttons:");
            for name in buttons.iter() {
                info!("  {name}");
            }
        }
        asyn::timeout(1.).with_result(Repeat::forever())
    }));
    commands.spawn(TextFont::from_font_size(30.0));
    commands
        .spawn((Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            justify_content: JustifyContent::SpaceAround,
            align_content: AlignContent::SpaceAround,
            flex_direction: FlexDirection::Column,
            ..default()
        },))
        .with_children(|parent| {
            for i in 0..16 {
                parent.spawn((
                    Button,
                    Node {
                        width: Val::Px(150.),
                        height: Val::Px(50.),
                        margin: UiRect::all(Val::Px(10.)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
                    Name::new(format!("Button {}", i)),
                ));
            }
        });
}
