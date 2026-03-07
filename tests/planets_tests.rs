mod planet_tests {
    use crate::planet_creators;
    use common_game::components::planet::Planet;
    use common_game::components::resource::BasicResourceType;
    use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
    use common_game::protocols::planet_explorer::{ExplorerToPlanet, PlanetToExplorer};
    use common_game::utils::ID;
    use crossbeam_channel::{Receiver, Sender};
    use std::fmt::Debug;
    use std::thread;
    use std::time::Duration;

    type CreatePlanet = fn(
        ID,
        Receiver<OrchestratorToPlanet>,
        Sender<PlanetToOrchestrator>,
        Receiver<ExplorerToPlanet>,
    ) -> Result<Planet, String>;

    fn send_and_receive<T: Debug, S>(sender: &Sender<T>, receiver: &Receiver<S>, msg: T) -> S {
        let s = format!("{msg:?}");
        sender
            .send(msg)
            .unwrap_or_else(|_| panic!("Failed to send message: {s}"));
        receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or_else(|_| panic!("Failed to receive message after sending: {s}"))
    }

    /// Tests a single Planet. Takes as input the function to create it.
    fn test_planet(create_planet: CreatePlanet) {
        // 0. Create channels
        let (tx_otp, rx_otp) = crossbeam_channel::unbounded::<OrchestratorToPlanet>();
        let (tx_pto, rx_pto) = crossbeam_channel::unbounded::<PlanetToOrchestrator>();
        let (tx_etp, rx_etp) = crossbeam_channel::unbounded::<ExplorerToPlanet>();
        let (tx_pte, rx_pte) = crossbeam_channel::unbounded::<PlanetToExplorer>();

        // 0. Create Planet
        let mut planet = create_planet(1, rx_otp, tx_pto, rx_etp).expect("Failed to create Planet");
        let handle = thread::spawn(move || {
            planet.run().unwrap();
        });

        // 1. Send StartPlanetAI, expect StartPlanetAIResult
        let response = send_and_receive(&tx_otp, &rx_pto, OrchestratorToPlanet::StartPlanetAI);
        assert!(
            matches!(response, PlanetToOrchestrator::StartPlanetAIResult { .. }),
            "Failed to receive StartPlanetAIResult"
        );

        // 2. Send IncomingExplorerRequest, expect IncomingExplorerResult
        let response = send_and_receive(
            &tx_otp,
            &rx_pto,
            OrchestratorToPlanet::IncomingExplorerRequest {
                explorer_id: 2,
                new_sender: tx_pte,
            },
        );
        assert!(
            matches!(
                response,
                PlanetToOrchestrator::IncomingExplorerResponse { .. }
            ),
            "Failed to receive IncomingExplorerResponse"
        );

        // 3. Explorer sends SupportedResourceRequest, expects SupportedResourceResponse
        let response = send_and_receive(
            &tx_etp,
            &rx_pte,
            ExplorerToPlanet::SupportedResourceRequest { explorer_id: 2 },
        );
        assert!(
            matches!(response, PlanetToExplorer::SupportedResourceResponse { .. }),
            "Failed to receive SupportedResourceResponse"
        );

        // 4. Explorer sends SupportedCombinationRequest, expects SupportedCombinationResponse
        let response = send_and_receive(
            &tx_etp,
            &rx_pte,
            ExplorerToPlanet::SupportedCombinationRequest { explorer_id: 2 },
        );
        assert!(
            matches!(
                response,
                PlanetToExplorer::SupportedCombinationResponse { .. }
            ),
            "Failed to receive SupportedCombinationResponse"
        );

        // 5. Explorer sends GenerateResourceRequest, expects GenerateResourceResponse
        let response = send_and_receive(
            &tx_etp,
            &rx_pte,
            ExplorerToPlanet::GenerateResourceRequest {
                explorer_id: 2,
                resource: BasicResourceType::Carbon,
            },
        );
        assert!(
            matches!(response, PlanetToExplorer::GenerateResourceResponse { .. }),
            "Failed to receive GenerateResourceResponse"
        );

        // 6. Send KillPlanet, expect KillPlanetResult
        let response = send_and_receive(&tx_otp, &rx_pto, OrchestratorToPlanet::KillPlanet);
        assert!(
            matches!(response, PlanetToOrchestrator::KillPlanetResult { .. },),
            "Failed to receive KillPlanetResult"
        );

        handle.join().expect("Failed to join Planet thread");
    }

    #[test]
    fn test_trip() {
        test_planet(planet_creators::create_trip_planet);
    }
    #[test]
    fn test_houston_we_have_a_borrow() {
        test_planet(planet_creators::create_houston_we_have_a_borrow_planet);
    }
    #[test]
    fn test_enterprise() {
        test_planet(planet_creators::create_enterprise_planet);
    }
    #[test]
    fn test_rustrelli() {
        test_planet(planet_creators::create_rustrelli_planet);
    }
    #[test]
    fn test_luna4() {
        test_planet(planet_creators::create_luna4_planet);
    }
    #[test]
    fn test_rusty_crab() {
        test_planet(planet_creators::create_rusty_crab_planet);
    }
    #[test]
    fn test_orbitron() {
        test_planet(planet_creators::create_orbitron_planet);
    }
}

mod planet_creators {
    use common_game::components::planet::Planet;
    use common_game::protocols::orchestrator_planet::{OrchestratorToPlanet, PlanetToOrchestrator};
    use common_game::protocols::planet_explorer::ExplorerToPlanet;
    use common_game::utils::ID;
    use crossbeam_channel::{Receiver, Sender};
    use houston_we_have_a_borrow::{RocketStrategy, houston_we_have_a_borrow};
    use trip::trip;

    pub(crate) fn create_trip_planet(
        id: ID,
        rx_orchestrator: Receiver<OrchestratorToPlanet>,
        tx_orchestrator: Sender<PlanetToOrchestrator>,
        rx_explorer: Receiver<ExplorerToPlanet>,
    ) -> Result<Planet, String> {
        trip(id, rx_orchestrator, tx_orchestrator, rx_explorer)
    }

    // Wrapper allowed to return the same type for all planet creation functions
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn create_rustrelli_planet(
        id: ID,
        rx_orchestrator: Receiver<OrchestratorToPlanet>,
        tx_orchestrator: Sender<PlanetToOrchestrator>,
        rx_explorer: Receiver<ExplorerToPlanet>,
    ) -> Result<Planet, String> {
        Ok(rustrelli::create_planet(
            id,
            rx_orchestrator,
            tx_orchestrator,
            rx_explorer,
            rustrelli::ExplorerRequestLimit::FairShare,
        ))
    }

    pub(crate) fn create_luna4_planet(
        id: ID,
        rx_orchestrator: Receiver<OrchestratorToPlanet>,
        tx_orchestrator: Sender<PlanetToOrchestrator>,
        rx_explorer: Receiver<ExplorerToPlanet>,
    ) -> Result<Planet, String> {
        luna4::create_planet(id, rx_orchestrator, tx_orchestrator, rx_explorer)
    }

    // Wrapper allowed to return the same type for all planet creation functions
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn create_rusty_crab_planet(
        id: ID,
        rx_orchestrator: Receiver<OrchestratorToPlanet>,
        tx_orchestrator: Sender<PlanetToOrchestrator>,
        rx_explorer: Receiver<ExplorerToPlanet>,
    ) -> Result<Planet, String> {
        Ok(rusty_crab_ap2025::planet::create_planet(
            rx_orchestrator,
            tx_orchestrator,
            rx_explorer,
            id,
        ))
    }

    // Wrapper allowed to return the same type for all planet creation functions
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn create_enterprise_planet(
        id: ID,
        rx_orchestrator: Receiver<OrchestratorToPlanet>,
        tx_orchestrator: Sender<PlanetToOrchestrator>,
        rx_explorer: Receiver<ExplorerToPlanet>,
    ) -> Result<Planet, String> {
        Ok(enterprise::create_planet(
            id,
            rx_orchestrator,
            tx_orchestrator,
            rx_explorer,
        ))
    }

    // Wrapper allowed to return the same type for all planet creation functions
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn create_orbitron_planet(
        id: ID,
        rx_orchestrator: Receiver<OrchestratorToPlanet>,
        tx_orchestrator: Sender<PlanetToOrchestrator>,
        rx_explorer: Receiver<ExplorerToPlanet>,
    ) -> Result<Planet, String> {
        Ok(orbitron::create_planet(
            rx_orchestrator,
            tx_orchestrator,
            rx_explorer,
            id,
        ))
    }

    pub(crate) fn create_houston_we_have_a_borrow_planet(
        id: ID,
        rx_orchestrator: Receiver<OrchestratorToPlanet>,
        tx_orchestrator: Sender<PlanetToOrchestrator>,
        rx_explorer: Receiver<ExplorerToPlanet>,
    ) -> Result<Planet, String> {
        houston_we_have_a_borrow(
            rx_orchestrator,
            tx_orchestrator,
            rx_explorer,
            id,
            RocketStrategy::Default,
            None,
        )
    }
}
