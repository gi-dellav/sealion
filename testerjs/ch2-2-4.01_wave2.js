import { heart, stack, beside, flip_vert, show } from 'rune';

const heart2 = beside(heart, flip_vert(heart));
// const heart4 = stack(heart2, heart2);

show(heart2);
// show(heart4);
