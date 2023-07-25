//! This example shows how to use `pecs` for organizing UI logic
//! with async operations. We create `exit` button that shows
//! confirmation popup on press and exit app if confirmed.
//!
//! The promise-based loop works like this:
//! - create exit button
//! - loop:     <-------------------------.
//!   - wait for exit button pressed      |
//!   - create popup with yes/no buttons  |
//!   - wait for yes or no pressed        |
//!   - repeat if no pressed -------------`
//!   - break loop if yes pressed --------.
//! - exit app  <-------------------------`
use bevy::prelude::*;
use pecs::prelude::*;

const COLOR_DARK: Color = Color::rgb(0.2, 0.2, 0.2);
const COLOR_LIGHT: Color = Color::rgb(0.8, 0.8, 0.8);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PecsPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
    let root = commands
        .spawn(NodeBundle {
            style: Style {
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                ..default()
            },
            ..default()
        })
        .id();
    commands.add(GameState::start(root));
}

#[derive(Clone, Copy)]
struct GameState {
    /// root entity, put other UI here
    root: Entity,
    /// exit button entity
    exit: Entity,
    /// current confiramtion popup entity
    popup: Option<Entity>,
}
impl GameState {
    /// Create the promise-based game loop
    fn start(root: Entity) -> Promise<(), ()> {
        Promise::from(root)
            // Create exit button and UiState that will be passed
            // through promises chain as `this` (just like self)
            .then(asyn!(state, mut commands: Commands, assets: Res<AssetServer> => {
                let root = state.value;
                let exit = add_button("Exit", &mut commands, &assets);
                commands.entity(root).add_child(exit);
                state.with(GameState { root, exit, popup: None })
            }))
            // asyn!(this => {
            //  loop {
            //      asyn::ui::pressed(this.exit).await;
            //      if this.ask_for_exit().await { break }
            //  }
            //  asyn::app::exit()
            // })
            // this is the loop
            .then_repeat(asyn!(this => {
                let exit = this.exit; //    <------------------------------.
                this.asyn()                                             // |
                    // wait for exit button pressed                     // |
                    .ui().button(exit).pressed()                        // |
                    // show popup and wait an answer                    // |
                    .then(asyn!(this => {                               // |
                        info!("Exit pressed");                          // |
                        this.ask_for_exit()                             // |
                    }))                                                 // |
                    .then(asyn!(this, confirmed => {                    // |
                        info!("Exit confirmed: {confirmed}");           // |
                        if !confirmed {                                 // |
                            // repeat the iteration if user presses no  // |
                            this.resolve(Repeat::Continue)  // ------------`
                        } else {
                            // break the loop if user presses yes
                            this.resolve(Repeat::Break(())) // ------------.
                        }                                               // |
                    }))                                                 // |
            })) //                                                      // |
            // the next promise will be called after previous           // |
            // `then_repeat` resolves with Repeat::Break                // |
            .then(asyn! {   //  <------------------------------------------`
                info!("Closing app");
                asyn::app::exit()
            })
    }
    /// Create Promise that adds popup with text and yes/no buttons
    /// waits for one of this button got pressed and resolve with
    /// - true if yes pressed
    /// - false if no pressed
    fn ask_for_exit(self) -> Promise<GameState, bool> {
        // create new promise from self state, it will be passed over chain call
        Promise::from(self)
            .then(asyn!(this, mut commands: Commands, assets: Res<AssetServer> => {
                // add popup as child to this.root, save popup entity at this.popup
                let (yes, no) = this.show_popup("Exit now?", &mut commands, &assets);
                // this.any() will be resolved when one of the passed promises got resolved
                this.any((
                    asyn::ui::button(yes).pressed(),
                    asyn::ui::button(no).pressed(),
                ))
            }))
            .then(asyn!(this, (yes, _no), mut commands: Commands => {
                // remove popup
                if let Some(popup) = this.popup {
                    commands.entity(popup).despawn_recursive();
                }
                this.popup = None;
                // and reolve with true/false
                this.resolve(yes.is_some())
            }))
    }

    fn show_popup(
        &mut self,
        text: &'static str,
        commands: &mut Commands,
        asset_server: &Res<AssetServer>,
    ) -> (Entity, Entity) {
        let yes = add_button("Yes", commands, asset_server);
        let no = add_button("No", commands, asset_server);
        commands.entity(self.root).with_children(|parent| {
            self.popup = Some(
                parent
                    .spawn(NodeBundle {
                        background_color: COLOR_LIGHT.into(),
                        style: Style {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(25.),
                            right: Val::Percent(25.),
                            top: Val::Percent(25.),
                            bottom: Val::Percent(25.),
                            ..default()
                        },
                        ..default()
                    })
                    .with_children(|popup| {
                        popup
                            .spawn(NodeBundle {
                                style: Style {
                                    width: Val::Percent(100.),
                                    height: Val::Percent(100.),
                                    flex_direction: FlexDirection::Column,
                                    align_content: AlignContent::Center,
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::SpaceAround,
                                    ..default()
                                },
                                ..default()
                            })
                            .with_children(|layout| {
                                layout.spawn(TextBundle::from_section(
                                    text,
                                    TextStyle {
                                        font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                                        font_size: 40.0,
                                        color: COLOR_DARK.into(),
                                    },
                                ));
                                let mut buttons = layout.spawn(NodeBundle {
                                    style: Style {
                                        flex_direction: FlexDirection::Row,
                                        justify_content: JustifyContent::SpaceAround,
                                        width: Val::Percent(100.),
                                        height: Val::Auto,
                                        ..default()
                                    },
                                    ..default()
                                });
                                buttons.add_child(yes);
                                buttons.add_child(no);
                            });
                    })
                    .id(),
            );
        });

        (yes, no)
    }
}

fn add_button(text: &'static str, commands: &mut Commands, asset_server: &Res<AssetServer>) -> Entity {
    commands
        .spawn(ButtonBundle {
            style: Style {
                width: Val::Px(150.0),
                height: Val::Px(65.0),
                margin: UiRect::all(Val::Auto),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            background_color: COLOR_DARK.into(),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn(TextBundle::from_section(
                text,
                TextStyle {
                    font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                    font_size: 40.0,
                    color: COLOR_LIGHT,
                },
            ));
        })
        .id()
}
