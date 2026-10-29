function integers_starting_from(n) {
    return pair(n, () => integers_starting_from(n + 1));
}
function scale_stream(stream, factor) {
    return stream_map(x => x * factor,
                      stream);
}
const positive_integers = integers_starting_from(1);
const multiples_of_three = scale_stream(positive_integers, 3);	
const multiples_of_five = scale_stream(positive_integers, 5);
const threes_and_fives = merge(multiples_of_three, multiples_of_five);
stream_ref(threes_and_fives, 50);
