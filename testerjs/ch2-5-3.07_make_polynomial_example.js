const p1 = make_polynomial("x",	
               list(list(2, make_javascript_number(4)),
                    list(1, make_javascript_number(3)),
                    list(0, make_javascript_number(7))));
const p2 = make_polynomial("x",	
               list(list(2, make_javascript_number(5)),
                    list(1, make_javascript_number(2)),
                    list(0, make_javascript_number(10))));

head(tail(tail(tail(mul(p1, p2)))));
