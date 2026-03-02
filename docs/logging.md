# LOGGING LEVELS

## `error`
An error is something that shouldn't happen. If it does, it breaks the game.

1. Planet creation fails.
2. Error inside planet thread loop.
3. Conversation timed out.

## `warn`
A warning is something that shouldn't happen, but it doesn't break the game.

1. Something should be inside a data structure, but it's not found.
2. Errors while receiving messages from entities.
3. Receiving messages from entities that don't start a conversation.
4. Poison recovery.
5. No planets available to perform a certain action.
6. Conversations reaching error state.

## `info`
1. Shutting down the orchestrator (no explorers left).
2. Switching game mode (pausing or resuming the game).
3. Creating explorers, or starting/stopping/resetting AI.
4. Explorer moved.
5. Planet destroyed by an asteroid.

## `debug`
1. Planet creation.
2. Receiving messages from entities.
3. Thread ended correctly.
4. Sending messages to entities/UI.
5. Planet sends a rocket to destroy an asteroid.
6. [LogTarget] Receiving messages from entities.

## `trace`
1. Conversation scheduling, matches, and transitions (especially since everything appears to work correctly).
2. Sending asteroids/sunrays (they work but are sent frequently).
3. Planet handles dead explorer.
4. Sunray received.
5. [LogTarget] Sending messages to entities (verified working).

## `off`
Logging disabled. 