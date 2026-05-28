//TODO: LOOK IF EXPECETED_MSG CAN BE REMOVED FROM THE STATE STRUCT, SINCE IT'S ALWAYS NONE FOR THIS KIND OF STATES. IF SO, ALSO REMOVE IT FROM THE MACRO PARAMETERS

/// A macro for defining conversation "request" states.
///
/// Request states are meant to execute an action (e.g., sending a message) without
/// waiting for an incoming message. Thus, they usually expect `None` for the message.
///
/// This macro generates a struct for the state, implements `ChannelsContext` to
/// give it access to communication channels, and implements `Conversation` for the
/// wrapper conversation type `$conv<$state>`.
///
/// The macro automatically requires the state to have an `orch_context` field of type `OrchContextRef`, that contains the necessary context for the state to operate.
///
/// # Parameters
/// * `state_name` - The identifier of the state struct to be generated.
/// * `conv_name` - The wrapper conversation type that will hold this state.
/// * `convo_kind` - The conversation kind used to derive execution priority.
/// * `timeout` - An expression yielding an `Option<Duration>` for the state's timeout.
/// * `expected_msg` - Typically `None` since this state is not waiting for a message.
/// * `fields` - A block defining the specific fields of the generated state struct.
/// * `entities_id_fn` - A closure mapping `&self` to an `EntitiesIDTuple` returning the IDs of the entities involved in the state.
/// * `transition_fn` - A transition function taking `Box<Self>` and returning the next state.
/// * `methods_settings` - key value pairs for specific behaviors that deviate from the default ones specified in the `Conversation` trait.
#[macro_export]
macro_rules! create_request_state {
    (
        state_name: $state:ident,
        conv_name: $conv:ident,
        convo_kind: $kind:expr,
        timeout: $timeout:expr,
        expected_msg: $expected_msg:expr,
        fields: { $($field:ident : $type:ty),* $(,)? }, //Takes the field specified and builds the state struct
        entities_id_fn: $entities_id_logic:expr,
        transition_fn:  $transition_logic:expr, //Takes a closure that is the transition function fo the state
        methods_settings: { $($key:ident : $logic:expr),* },

    ) => {
        pub(crate) struct $state {
            expected_msg: Option<PossibleExpectedKinds>,
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
            fn get_expected_kind(&self) -> Option<PossibleExpectedKinds> { $expected_msg }
            fn get_entities_ids(&self) -> EntitiesIDTuple {
                ($entities_id_logic) (self)
            }
            fn transition(
                self: Box<Self>,
                msg: Option<PossibleMessage>,
            ) -> Option<Box<dyn Conversation + Send + Sync>> {
                if msg.is_some() {
                    let error_state = ErrorState::new(Box::new(CommonErrorTypes::WrongMessage), self.id);
                    return Some(Box::new(error_state) as Box<dyn Conversation + Send + Sync>);
                }
                ($transition_logic) (self)
            }
            $( $crate::conversation_settings_dispatcher!($key, $logic); )* //Takes eventual specific behaviors
        }
    };
}
