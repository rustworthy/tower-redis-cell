use crate::error::Error;
use crate::rule::RequestAllowedDetails;

pub(crate) type SyncSuccessHandler<RespTy> =
    Box<dyn Fn(RequestAllowedDetails, &mut RespTy) + Send + Sync + 'static>;

pub(crate) type SyncUnruledHandler<RespTy> = Box<dyn Fn(&mut RespTy) + Send + Sync + 'static>;

pub(crate) type SyncErrorHandler<ReqTy, IntoRespTy> =
    Box<dyn Fn(Error, &ReqTy) -> IntoRespTy + Send + Sync + 'static>;

pub(crate) enum OnSuccess<RespTy> {
    Noop,
    Sync(SyncSuccessHandler<RespTy>),
}

pub(crate) enum OnUnruled<RespTy> {
    Noop,
    Sync(SyncUnruledHandler<RespTy>),
}

pub(crate) enum OnError<ReqTy, IntoRespTy> {
    Sync(SyncErrorHandler<ReqTy, IntoRespTy>),
}

pub struct RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy> {
    pub(crate) rule_provider: PR,
    pub(crate) on_error: OnError<ReqTy, IntoRespTy>,
    pub(crate) on_success: OnSuccess<RespTy>,
    pub(crate) on_unruled: OnUnruled<RespTy>,
}

impl<RP, ReqTy, RespTy, IntoRespTy> RateLimitConfig<RP, ReqTy, RespTy, IntoRespTy> {
    pub fn new<EH>(rule_provider: RP, error_handler: EH) -> Self
    where
        EH: Fn(Error, &ReqTy) -> IntoRespTy + Send + Sync + 'static,
    {
        RateLimitConfig {
            rule_provider,
            on_error: OnError::Sync(Box::new(error_handler)),
            on_success: OnSuccess::Noop,
            on_unruled: OnUnruled::Noop,
        }
    }

    pub fn on_success<H>(mut self, handler: H) -> Self
    where
        H: Fn(RequestAllowedDetails, &mut RespTy) + Send + Sync + 'static,
    {
        self.on_success = OnSuccess::Sync(Box::new(handler));
        self
    }

    pub fn on_unruled<H>(mut self, handler: H) -> Self
    where
        H: Fn(&mut RespTy) + Send + Sync + 'static,
    {
        self.on_unruled = OnUnruled::Sync(Box::new(handler));
        self
    }
}
