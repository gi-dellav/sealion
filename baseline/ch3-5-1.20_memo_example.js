let calls = 0;
function square_4() {
    const result = 4 * 4;	    
    calls = calls + 1;
    return result;
}
const memo_square_4 = memo(square_4);
memo_square_4(); 
memo_square_4();
calls;
