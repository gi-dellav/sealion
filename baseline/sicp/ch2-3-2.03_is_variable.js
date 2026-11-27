import { is_string } from './sicp.js';
function is_variable(x) { return is_string(x); }

is_variable("xyz");

// expected: true
