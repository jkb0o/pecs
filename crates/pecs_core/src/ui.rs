use bevy::prelude::*;

use crate::{AsynOps, Promise, PromiseCommandsExtension, PromiseId, PromiseLikeBase};

pub mod asyn {
    use super::AsynButton;
    use bevy::prelude::Entity;

    pub fn button(entity: Entity) -> AsynButton {
        AsynButton(entity)
    }
}

pub struct PromiseUiPlugin;
impl Plugin for PromiseUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(resolve_buttons);
    }
}

pub struct StatefulAsynUi<S>(S);
impl<S: 'static> StatefulAsynUi<S> {
    pub fn button(self, entity: Entity) -> StatefulAsynButton<S> {
        StatefulAsynButton(self.0, entity)
    }
}

#[derive(Component)]
pub struct AsynButtonIteraction {
    promise: PromiseId,
    interaction: Interaction,
    entity: Entity,
}

pub struct AsynButton(Entity);

impl AsynButton {
    pub fn pressed(&self) -> Promise<(), ()> {
        let entity = self.0;
        Promise::register(
            move |world, id| {
                world.spawn(AsynButtonIteraction {
                    entity,
                    promise: id,
                    interaction: Interaction::Clicked,
                });
            },
            move |world, id| {
                if let Some(despawn) = world
                    .query::<(Entity, &AsynButtonIteraction)>()
                    .iter(world)
                    .filter(|(_, b)| b.promise == id)
                    .map(|(e, _)| e)
                    .next()
                {
                    world.despawn(despawn);
                }
            },
        )
    }
}

pub struct StatefulAsynButton<S>(S, Entity);
impl<S: 'static> StatefulAsynButton<S> {
    pub fn pressed(self) -> Promise<S, ()> {
        AsynButton(self.1).pressed().with(self.0)
    }
}

pub trait UiOpsExtension<S> {
    fn ui(self) -> StatefulAsynUi<S>;
}
impl<S: 'static> UiOpsExtension<S> for AsynOps<S> {
    fn ui(self) -> StatefulAsynUi<S> {
        StatefulAsynUi(self.0)
    }
}

fn resolve_buttons(
    mut commands: Commands,
    buttons: Query<(Entity, &AsynButtonIteraction)>,
    interactions: Query<(Entity, &Interaction), (Changed<Interaction>, With<Button>)>,
) {
    for (btn, interaction) in interactions.iter() {
        if let Some((entity, btn)) = buttons
            .iter()
            .filter(|(_, b)| b.entity == btn && interaction == &b.interaction)
            .next()
        {
            commands.entity(entity).despawn();
            commands.promise(btn.promise).resolve(())
        }
    }
}
