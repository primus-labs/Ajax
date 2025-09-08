#[repr(C)]
pub(crate) struct PrimalLpnParameterWrapper {
    _private: [u8; 0],
}

extern "C" {
    pub(crate) fn new_primal_lpn_parameter(
        n: i64,
        t: i64,
        k: i64,
        log_bin_sz: i64,
        n_pre: i64,
        t_pre: i64,
        k_pre: i64,
        log_bin_sz_pre: i64,
    ) -> *mut PrimalLpnParameterWrapper;

    pub(crate) fn delete_primal_lpn_parameter(param: *mut PrimalLpnParameterWrapper);
}

pub struct PrimalLpnParameter {
    pub(crate) param: *mut PrimalLpnParameterWrapper,
}

impl PrimalLpnParameter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        n: i64,
        t: i64,
        k: i64,
        log_bin_sz: i64,
        n_pre: i64,
        t_pre: i64,
        k_pre: i64,
        log_bin_sz_pre: i64,
    ) -> Self {
        let param = unsafe {
            new_primal_lpn_parameter(n, t, k, log_bin_sz, n_pre, t_pre, k_pre, log_bin_sz_pre)
        };
        Self { param }
    }
}

impl Drop for PrimalLpnParameter {
    fn drop(&mut self) {
        unsafe {
            delete_primal_lpn_parameter(self.param);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_lpn_parameter() {
        let t: i64 = 3;
        let k: i64 = 1;
        let log_bin_sz: i64 = 4;
        let n: i64 = t * (1 << log_bin_sz);
        let t_pre: i64 = 5;
        let k_pre: i64 = 3;
        let log_bin_sz_pre: i64 = 8;
        let n_pre: i64 = t_pre * (1 << log_bin_sz_pre);

        assert!(n_pre >= k + t * log_bin_sz + 128);

        let param =
            PrimalLpnParameter::new(n, t, k, log_bin_sz, n_pre, t_pre, k_pre, log_bin_sz_pre);
        drop(param);
    }
}
