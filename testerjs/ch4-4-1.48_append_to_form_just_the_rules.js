process_query(`assert(
rule(append_to_form(null, $y, $y)))`);
process_query(`assert(
rule(append_to_form(pair($u, $v), $y, pair($u, $z)),
     append_to_form($v, $y, $z)))`);
