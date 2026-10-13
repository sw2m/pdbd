// pdbd.v1 wire contract — see README "Control plane".
pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/pdbd.v1.rs"));
}
