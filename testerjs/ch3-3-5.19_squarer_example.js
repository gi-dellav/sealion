const a = make_connector();
const b = make_connector();
squarer(a, b);
probe("a", a);
probe("b", b);
set_value(a, 3, "user");
