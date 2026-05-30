pub(crate) mod request_state;
pub(crate) mod response_state;

//Macro for defining tha wrapper structure of each conversation flow
#[macro_export]
macro_rules! define_conversation {
    (name: $name:ident ) => {
        pub(crate) struct $name<State> {
            id: common_game::utils::ID,
            state: State,
        }

        impl<State: Send + Sync> $name<State> {
            pub fn new(id: common_game::utils::ID, state: State) -> Self {
                Self { id, state }
            }
        }
    };
}

//Macro to look for specific behaviors in the settings block, overriding the default behaviors of the trait
#[macro_export]
macro_rules! conversation_settings_dispatcher {
    (error_details, $logic:expr) => {
        fn get_error_details(&self) -> Option<String> {
            ($logic)(self)
        }
    };
    (get_kill_exp_vec, $logic:expr) => {
        fn get_kill_explorers_vec(&self) -> Option<(KillExplorersList, bool)> {
            ($logic)(self)
        }
    };

    (on_timeout, $logic:expr) => {
        fn on_timeout(self: Box<Self>) {
            ($logic)(self)
        }
    };
}
