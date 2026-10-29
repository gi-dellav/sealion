function attach_tag(type_tag, contents) {
    return pair(type_tag, contents);
}
function type_tag(datum) {
    return is_pair(datum)
           ? head(datum)
           : error(datum, "bad tagged datum -- type_tag");
}
function contents(datum) {
    return is_pair(datum)
           ? tail(datum)
           : error(datum, "bad tagged datum -- contents");
}
function square(x) {
    return x * x;
}
function real_part_polar(z) {
    return magnitude_polar(z) * math_cos(angle_polar(z));
}
function imag_part_polar(z) {
    return magnitude_polar(z) * math_sin(angle_polar(z));
}
function magnitude_polar(z) { return head(z); }

function angle_polar(z) { return tail(z); }

function make_from_real_imag_polar(x, y) {
    return attach_tag("polar",
                      pair(math_sqrt(square(x) + square(y)),
                           math_atan2(y, x)));
}

function make_from_mag_ang_polar(r, a) {
    return attach_tag("polar", pair(r, a));
}

const alyssas_co_num = make_from_mag_ang_polar(
                           3.0, 0.7);

imag_part_polar(contents(alyssas_co_num));

// expected: 1.932653061713073
