import { parse } from './sicp.js';
// null is the empty environment (not used here)	
analyze_literal(parse("true;"))(null);
