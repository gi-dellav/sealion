const a = make_connector();
const b = make_connector();
const c = make_connector();
averager(a, b, c);

probe("c", c);
probe("b", b);
probe("a", a);

set_value(a, 25, "user");
set_value(c, 125, "user");
forget_value(a, "user");
forget_value(c, "user");
set_value(a, 7, "user");
set_value(b, 13, "user");
