mod request_state;
mod response_state;


//Macro for defining tha wrapper structure of each conversation flow
#[macro_export]
macro_rules! define_conversation {
    (name: $name:ident, expected_msg:$expected:expr ) => {
        pub(crate) struct $name<State> {
            id: ID,
            expected_message: Option<PossibleExpectedKinds>,
            state: State,
        }

        impl<State: Send + Sync> $name<State> {
            pub fn new(id: ID, state: State) -> Self {
                Self { id, state, expected_message: $expected }
            }
        }
    };
}


//Macro to look for specific behaviors in the settings block
#[macro_export]
macro_rules! conversation_settings_dispatcher {

    (get_timeout, $logic:block) => {
        fn get_timeout(&self) -> Option<Duration> { let this = self; $logic }
    };
    (error_details, $logic:block) => {
        fn get_error_details(&self) -> Option<String> { let this = self; $logic }
    };
    (get_kill_exp_vec, $logic:block) => {
        fn get_kill_explorers_vec(&self) -> Option<(KillExplorersList, bool)> { let this = self; $logic }
    };
    (on_timeout, $logic:block) => {
        fn on_timeout(self: Box<Self>) { let this = self; $logic }
    };

}