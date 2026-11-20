import { head, list, tail } from './sicp.js';
head(tail(head(tail(head(tail(tail(deriv(list("**", "x", 4), "x"))))))));
