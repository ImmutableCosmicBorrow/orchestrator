//Macro for defining request states (don't expect any message)
#[macro_export]
macro_rules! create_request_state {
    (
        state_name: $state:ident,
        conv_name: $conv:ident,
        priority: $pri:expr,
        fields: { $($field:ident : $type:ty),* $(,)? },//Takes the field specified and builds the state struct
        entities_id_closure: $entities_id_logic:block,
        transition_closure:  $transition_logic:block,//Takes a closure that is the transition function fo the state
        methods_settings: { $($key:ident : $logic:block),* },

    ) => {
        pub(crate) struct $state { $($field: $type),* }

        impl Conversation for $conv<$state> {
            fn get_id(&self) -> ID { self.id }
            fn get_priority(&self) -> i32 { $pri }
            fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> { self.expected_message.clone() }
            fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
                let this = self;
                $entities_id_logic
            }
            fn transition(
                self: Box<Self>,
                _msg: Option<PossibleMessage<ExplorerBagContent>>,
            ) -> Option<Box<dyn Conversation>> {
                let this = self;
                $transition_logic
            }
            $( $crate::conversation_settings_dispatcher!($key, $logic); )* //Takes eventual specific behaviors
        }
    };
}