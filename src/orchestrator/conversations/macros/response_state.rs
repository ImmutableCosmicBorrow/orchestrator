#[macro_export]
macro_rules! create_response_state {
    (
        state: $state:ident,
        conv: $conv:ident,
        expected_variant: $variant:path,
        fields: { $($field:ident : $t:ty),* $(,)? },
        entities_id_closure: $entities_id_logic:block,
        settings: { $($key:ident : $logic:block),* $(,)? },
        transition: |$($msg:ident),*| $trans_logic:block
    ) => {
        pub(crate) struct $state { $($field: $t),* }

        impl Conversation for $conv<$state> {
            fn get_id(&self) -> ID { self.id }
            fn get_id(&self) -> ID { self.id }
            fn get_priority(&self) -> i32 { $pri }
            fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> { self.expected_message.clone() }
            fn get_entities_ids(&self) -> (Option<ID>, Option<ID>) {
                let this = self;
                $entities_id_logic
            }

            // Delegate metadata to the global dispatcher
            $( $crate::conversation_settings_dispatcher!($key, $logic); )*

            fn transition(
                self: Box<Self>,
                msg: Option<PossibleMessage>
            ) -> Option<Box<dyn Conversation>> {
                let this = self;
                $trans_logic
            }
        }
    };
}