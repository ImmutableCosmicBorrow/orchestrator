use chrono::Duration;
use crate::orchestrator::conversations::{PossibleExpectedKinds, PossibleMessage, ErrorState, CommonErrorTypes};
use crate::orchestrator::ExplorerBagContent;

//Macro for defining request states (don't expect any message)
#[macro_export]
macro_rules! create_request_state {
    (
        state_name: $state:ident,
        conv_name: $conv:ident,
        priority: $pri:expr,
        timeout: $timeout:expr,
        expected_msg: $expected_msg:expr,
        fields: { $($field:ident : $type:ty),* $(,)? },//Takes the field specified and builds the state struct
        entities_id_fn: $entities_id_logic:expr,
        transition_fn:  $transition_logic:expr,//Takes a closure that is the transition function fo the state
        methods_settings: { $($key:ident : $logic:expr),* },

    ) => {
        pub(crate) struct $state {
            expected_msg: Option<PossibleExpectedKinds>,
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
            fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> { $expected_msg }
            fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
                ($entities_id_logic) (self)
            }
            fn transition(
                self: Box<Self>,
                msg: Option<PossibleMessage>,
            ) -> Option<Box<dyn Conversation + Send + Sync>> {
                if msg.is_some() {
                    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
                    Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>);
                }
                ($transition_logic) (self)
            }
            $( $crate::conversation_settings_dispatcher!($key, $logic); )* //Takes eventual specific behaviors
        }
    };
}