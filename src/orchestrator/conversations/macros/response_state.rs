/// A macro for quickly defining conversation "response" states.
///
/// Response states are meant to wait for a specific incoming message from an entity
/// (e.g., an explorer or a planet) and process it to decide the next transition.
///
/// This macro generates a struct for the state, implements `ChannelsContext` to
/// give it access to communication channels, and implements `Conversation` for the
/// wrapper conversation type `$conv<$state>`.
///
/// # Parameters
/// * `state` - The identifier of the state struct to be generated.
/// * `conv` - The wrapper conversation type that will hold this state.
/// * `convo_kind` - The conversation kind used to derive execution priority.
/// * `timeout` - An expression yielding an `Option<Duration>` for the state's timeout.
/// * `expected_msg` - An expression yielding a `PossibleExpectedKinds` that this state expects.
/// * `fields` - A block defining the specific fields of the generated state struct.
/// * `entities_id_closure` - A closure mapping `&self` to an `EntitiesIDTuple`.
/// * `transition` - A transition function taking `Box<Self>` and `Option<PossibleMessage>`, returning the next state.
/// * `methods_settings` - Additional behaviors dispatched to other conversation traits.
#[macro_export]
macro_rules! create_response_state {
    (
        state: $state:ident,
        conv: $conv:ident,
        convo_kind: $kind:expr,
        timeout: $timeout:expr,
        expected_msg: $expected_msg:expr,
        fields: { $($field:ident : $type:ty),* $(,)? },
        entities_id_closure: $entities_id_logic:expr,
        transition: $trans_logic:expr,
        methods_settings: { $($key:ident : $logic:expr),* $(,)? },

    ) => {
        pub(crate) struct $state {
            expected_msg: PossibleExpectedKinds,
            orch_context: OrchContextRef,
            convo_kind: $crate::orchestrator::conversations::params::ConvoKind,
            $($field: $type),*
        }

        impl $state {
            pub(crate) fn new(orch_context: OrchContextRef, $($field: $type),*) -> Self {
                Self {
                    $($field,)*
                    expected_msg: $expected_msg,
                    orch_context,
                    convo_kind: $kind,
                }
            }
        }

        impl ChannelsContext for $state {
            fn get_channels_manager(&self) -> ChannelsManagerRef {
                self.orch_context.channels_manager.clone()
            }
        }

        impl Conversation for $conv<$state> {
            fn get_id(&self) -> ID { self.id }
            fn get_priority(&self) -> i32 { self.state.convo_kind.priority().as_i32() }
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
