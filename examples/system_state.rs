//! This example shows how promises keep the state of Bevy's system params.
//! We create 16 buttons and asyn loop single promise every second.
//! Inside the promise we log buttons with changed for the previous second
//! `Interaction` component by querying with Changed<Interaction> filter.
use bevy::prelude::*;
use pecs::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(PecsPlugin)
        .add_startup_system(setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
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
    commands
        .spawn(NodeBundle {
            style: Style {
                size: Size::new(Val::Percent(100.), Val::Percent(100.)),
                justify_content: JustifyContent::SpaceAround,
                align_content: AlignContent::SpaceAround,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ..default()
        })
        .with_children(|parent| {
            for y in 0..4 {
                parent
                    .spawn(NodeBundle {
                        style: Style {
                            size: Size::new(Val::Percent(100.), Val::Percent(20.)),
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::SpaceAround,
                            ..default()
                        },
                        ..default()
                    })
                    .with_children(|parent| {
                        for x in 0..4 {
                            parent
                                .spawn(ButtonBundle {
                                    background_color: Color::rgb(0.8, 0.8, 0.8).into(),
                                    style: Style {
                                        size: Size::new(Val::Percent(20.), Val::Percent(100.)),
                                        ..default()
                                    },
                                    ..default()
                                })
                                .insert(Name::new(format!("{x}x{y}")));
                        }
                    });
            }
        });
}
