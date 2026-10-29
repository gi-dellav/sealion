function integers_starting_from(n) {
    return pair(n, () => integers_starting_from(n + 1));
}
function scale_stream(stream, factor) {
    return stream_map(x => x * factor,
                      stream);
}
function merge(s1, s2) {
    if (is_null(s1)) {
        return s2;
    } else if (is_null(s2)) {
        return s1;
    } else {
        const s1head = head(s1);
        const s2head = head(s2);
        return s1head < s2head
               ? pair(s1head, () => merge(stream_tail(s1), s2))
               : s1head > s2head
               ? pair(s2head, () => merge(s1, stream_tail(s2)))
               : pair(s1head, () => merge(stream_tail(s1), stream_tail(s2)));
    }
}

const positive_integers = integers_starting_from(1);
const multiples_of_three = scale_stream(positive_integers, 3);	
const multiples_of_five = scale_stream(positive_integers, 5);
const threes_and_fives = merge(multiples_of_three, multiples_of_five);
stream_ref(threes_and_fives, 50);

// expected: 110
