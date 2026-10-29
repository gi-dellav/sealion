import { heart, stack, quarter_turn_left, quarter_turn_right, turn_upside_down, beside, show } from 'rune';

function identity(x) {
    return x;
}
function square_of_four(tl, tr, bl, br) {
    return painter => stack(beside(tl(painter), tr(painter)),
                            beside(bl(painter), br(painter)));
}

show(square_of_four(turn_upside_down, identity, 
                    quarter_turn_right, quarter_turn_left)
     (heart)
    );
