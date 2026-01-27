# Galaxy Setup Guide

This document explains how to create a galaxy configuration file for the orchestrator, with a focus on the ID system used to identify planets.

## Overview

The galaxy is defined by a single number in the galaxy configuration file (e.g., `galaxy/test_galaxy.txt`). This number encodes all the planets in the galaxy using a bitwise ID system.

## ID System Architecture

The orchestrator uses a bitwise ID system to uniquely identify different entities:

### Entity Types

IDs are structured using specific bit shifts to differentiate between entity types:

- **Conversations**: Use bit 16 (`CONVERSATION_SHIFT`)
- **Planets**: Use bit 15 (`PLANET_SHIFT`) as the base identifier
- **Explorers**: Use bit 7 (`EXPLORER_SHIFT`) as the base identifier

### Planet Type Identification

Each planet type has a specific bit that identifies its type. Planets combine the `PLANET_SHIFT` bit with their type-specific bit and a unique sequential number (0-15):

| Planet Type | Bit Position | Shift Constant |
|------------|--------------|----------------|
| **TRIP** | 14 | `TRIP_SHIFT` |
| **Rustrelli** | 13 | `RUSTRELLI_SHIFT` |
| **Luna4** | 12 | `LUNA4_SHIFT` |
| **Rusty Crab** | 11 | `RUSTY_CRAB_SHIFT` |
| **Enterprise** | 10 | `ENTERPRISE_SHIFT` |
| **Orbitron** | 9 | `ORBITRON_SHIFT` |
| **Houston** | 8 | `HOUSTON_SHIFT` |

### ID Formula

A planet ID is calculated as:
```
ID = (1 << PLANET_SHIFT) | (1 << TYPE_SHIFT) | sequential_number
```

Where `sequential_number` ranges from 1 to 255 (8 bits, allowing up to 256 planets per type).

## Example Planet IDs
If you're creating a galaxy, here there are several planets IDs, divided per groups, so that you don't need to calculate them by hand 

### TRIP Planet IDs
- **49153** = `0b1100000000000001` - TRIP Planet #1
- **49154** = `0b1100000000000010` - TRIP Planet #2
- **49155** = `0b1100000000000011` - TRIP Planet #3
- **49156** = `0b1100000000000100` - TRIP Planet #4

### Rustrelli Planet IDs
- **40961** = `0b1010000000000001` - Rustrelli Planet #1
- **40962** = `0b1010000000000010` - Rustrelli Planet #2
- **40963** = `0b1010000000000011` - Rustrelli Planet #3
- **40964** = `0b1010000000000100` - Rustrelli Planet #4

### Luna4 Planet IDs
- **36865** = `0b1001000000000001` - Luna4 Planet #1
- **36866** = `0b1001000000000010` - Luna4 Planet #2
- **36867** = `0b1001000000000011` - Luna4 Planet #3
- **36868** = `0b1001000000000100` - Luna4 Planet #4

### Rusty Crab Planet IDs
- **34817** = `0b1000100000000001` - Rusty Crab Planet #1
- **34818** = `0b1000100000000010` - Rusty Crab Planet #2
- **34819** = `0b1000100000000011` - Rusty Crab Planet #3
- **34820** = `0b1000100000000100` - Rusty Crab Planet #4

### Enterprise Planet IDs
- **33793** = `0b1000010000000001` - Enterprise Planet #1
- **33794** = `0b1000010000000010` - Enterprise Planet #2
- **33795** = `0b1000010000000011` - Enterprise Planet #3
- **33796** = `0b1000010000000100` - Enterprise Planet #4

### Orbitron Planet IDs
- **33281** = `0b1000001000000001` - Orbitron Planet #1
- **33282** = `0b1000001000000010` - Orbitron Planet #2
- **33283** = `0b1000001000000011` - Orbitron Planet #3
- **33284** = `0b1000001000000100` - Orbitron Planet #4

### Houston Planet IDs
- **33025** = `0b1000000100000001` - Houston Planet #1
- **33026** = `0b1000000100000010` - Houston Planet #2
- **33027** = `0b1000000100000011` - Houston Planet #3
- **33028** = `0b1000000100000100` - Houston Planet #4

## Limitations

- **Maximum 256 planets per type**: The ID system uses 8 bits for the sequential number (PLANET_MASK = 0b1111_1111)
- **Planet IDs must be unique**: Each planet must have a unique combination of type and sequential number
- **Type bits are exclusive**: Each planet belongs to exactly one type

### Explorer Subtypes

Explorer IDs combine the `EXPLORER_SHIFT` base bit (bit 7) with subtype bits and a 4-bit sequence (`EXPLORER_MASK = 0b1111`, up to 16 per subtype):

| Explorer Subtype | Bit Position | Shift Constant |
|------------------|--------------|----------------|
| **Base Explorer**| 7            | `EXPLORER_SHIFT` |
| **Nico**         | 6            | `NICO_SHIFT`   |
| **Jaco**         | 5            | `JACO_SHIFT`   |
| **Rob**          | 4            | `ROB_SHIFT`    |

ID formula for explorers:
```
ID = (1 << EXPLORER_SHIFT) | (1 << SUBTYPE_SHIFT) | (sequence & EXPLORER_MASK)
```

Example Explorer IDs:
- **Nico**: 193, 194, 195, 196 = `0b11000001`, `0b11000010`, `0b11000011`, `0b11000100`
- **Jaco**: 161, 162, 163, 164 = `0b10100001`, `0b10100010`, `0b10100011`, `0b10100100`
- **Rob**: 145, 146, 147, 148 = `0b10010001`, `0b10010010`, `0b10010011`, `0b10010100`

## Technical Details

The ID system is implemented in `src/id.rs` with the `IdManager` struct, which provides thread-safe ID generation using `Arc<Mutex<ID>>` for concurrent environments.
