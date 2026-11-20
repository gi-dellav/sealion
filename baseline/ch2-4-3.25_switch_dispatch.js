function tag_of(value) {
    switch (value) {
        case 1:
            return 10;
        case 2:
        case 3:
            return 20;
        default:
            return 0;
    }
}

function test(x, y) {
    return x === 0 ? 0 : y;
}

test(tag_of(1) + tag_of(2) + tag_of(3) + tag_of(9) - 50, 1);
