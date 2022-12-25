export const DEBUG_MODE = process.env.DEBUG_MODE || false;
export const DISPLAY_LOG = process.env.BIFROST_LOG || false;
export const BIFROST_LOG = process.env.BIFROST_LOG || 'info';

export const BINARY_PATH = process.env.BINARY_PATH || `../target/release/bifrost-node`;

// Is undefined by default as the path is dependent of the runtime.
export const OVERRIDE_RUNTIME_PATH = process.env['OVERRIDE_RUNTIME_PATH'] || undefined;
export const SPAWNING_TIME = 20000;

export const BASIC_MAXIMUM_OFFENCE_COUNT = 3;
export const FULL_MAXIMUM_OFFENCE_COUNT = 5;
