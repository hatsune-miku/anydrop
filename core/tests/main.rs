extern crate anydrop;

use anydrop::lib_generic;

#[test]
fn it_works() {
    assert!(lib_generic::anydrop_version() > 20230000);
    assert!(lib_generic::anydrop_version() < 20240000);
}
