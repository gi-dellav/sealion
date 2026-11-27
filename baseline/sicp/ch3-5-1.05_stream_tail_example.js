import { head, pair, stream, stream_tail, tail } from './sicp.js';
head(stream_tail(pair(4, () => pair(5, () => null))));
