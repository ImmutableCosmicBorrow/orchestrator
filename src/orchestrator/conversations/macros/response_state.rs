use chrono::Duration;
use crate::orchestrator::conversations::{EntitiesIDTuple, PossibleExpectedKinds};

#[macro_export]
macro_rules! create_response_state {
    (
        state: $state:ident,
        conv: $conv:ident,
        priority: $pri:expr,
        timeout: $timeout:expr,
        expected_msg: $expected_msg:expr,
        fields: { $($field:ident : $type:ty),* $(,)? },
        entities_id_closure: $entities_id_logic:expr,
        transition: $trans_logic:expr,
        methods_settings: { $($key:ident : $logic:expr),* $(,)? },

    ) => {
        pub(crate) struct $state {
            expected_msg: PossibleExpectedKinds,
            $($field: $type),*
        }

        impl $state {
            pub(crate) fn new($($field: $type),*) -> Self {
                Self {
                    $($field,)*
                    expected_msg: $expected_msg,
                }
            }
        }

        impl Conversation for $conv<$state> {
            fn get_id(&self) -> ID { self.id }
            fn get_priority(&self) -> i32 { $pri }
            fn get_timeout(&self) -> Option<Duration> { $timeout }
            fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> { Some($expected_msg) }
            fn get_entities_ids(&self) -> EntitiesIDTuple {
                ($entities_id_logic) (self)
            }

            // Delegate metadata to the global dispatcher
            $( $crate::conversation_settings_dispatcher!($key, $logic); )*

            fn transition(
                self: Box<Self>,
                msg: Option<PossibleMessage>
            ) -> Option<Box<dyn Conversation + Send + Sync>> {
                ($trans_logic) (self, msg)
            }
        }
    };
}