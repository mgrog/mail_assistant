use tower::ServiceBuilder;
use tower_http::{
    classify::{ServerErrorsAsFailures, SharedClassifier},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnRequest, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::Level;

pub fn request_id_layer() -> SetRequestIdLayer<MakeRequestUuid> {
    // set `x-request-id` header on all requests
    SetRequestIdLayer::x_request_id(MakeRequestUuid)
}

pub fn propagate_request_id_layer() -> PropagateRequestIdLayer {
    // propagate `x-request-id` header
    PropagateRequestIdLayer::x_request_id()
}

pub fn tracing_layer() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
        .make_span_with(
            DefaultMakeSpan::new()
                .include_headers(true)
                .level(Level::INFO),
        )
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(
            DefaultOnResponse::new()
                .level(Level::INFO)
                .latency_unit(LatencyUnit::Millis)
                .include_headers(true),
        )
        .on_failure(DefaultOnFailure::new().level(Level::ERROR))
}

type RequestTracingLayer = tower::ServiceBuilder<
    tower::layer::util::Stack<
        tower_http::request_id::PropagateRequestIdLayer,
        tower::layer::util::Stack<
            tower_http::trace::TraceLayer<
                tower_http::classify::SharedClassifier<
                    tower_http::classify::ServerErrorsAsFailures,
                >,
            >,
            tower::layer::util::Stack<
                tower_http::request_id::SetRequestIdLayer<tower_http::request_id::MakeRequestUuid>,
                tower::layer::util::Identity,
            >,
        >,
    >,
>;

pub fn trace_with_request_id_layer() -> RequestTracingLayer {
    ServiceBuilder::new()
        .layer(request_id_layer())
        .layer(tracing_layer())
        .layer(propagate_request_id_layer())
}
