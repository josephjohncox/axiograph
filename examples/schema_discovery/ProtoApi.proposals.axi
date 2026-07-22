-- Draft `.axi` module generated from `proposals.json`.
--
-- This output is a pre-vetted raw proposal example for teaching draft-module and
-- typed review flows. It is *untrusted* evidence-plane material, not accepted
-- ontology truth. Review before promotion.
--
-- Design notes:
-- - Entities become object inhabitants.
-- - Relations become binary tuples: `Rel(from, to)`.
-- - If proposals include a `context` attribute on relations, we preserve it:
--     - relation decls gain `@context Context`
--     - tuples add `ctx=...`
-- - Missing or heterogeneous endpoint types become explicit `TypeHole_*` review obligations.
-- - Optional constraints are inferred *extensionally* from current tuples.
--
-- Re-generate (from repo root):
--   PATH=/opt/homebrew/bin:$PATH cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- discover draft-module \
--     examples/schema_discovery/inputs/proto_api_proposals.json \
--     --out build/ProtoApi.proposals.axi \
--     --module ProtoApi_Proposals \
--     --schema ProtoApi \
--     --instance ProtoApiInstance \
--     --infer-constraints

module ProtoApi_Proposals

schema ProtoApi:

  -- Object types observed in proposals plus explicit typed holes.
  object ApiWorkflow
  object HttpEndpoint
  object ProtoMessage
  object ProtoRpc
  object ProtoService

  -- Binary relations observed in proposals.
  relation proto_rpc_http_endpoint(from: ProtoRpc, to: HttpEndpoint)
  relation proto_rpc_request(from: ProtoRpc, to: ProtoMessage)
  relation proto_rpc_response(from: ProtoRpc, to: ProtoMessage)
  relation proto_service_has_rpc(from: ProtoService, to: ProtoRpc)
  relation proto_service_has_workflow(from: ProtoService, to: ApiWorkflow)
  relation workflow_includes_rpc(from: ApiWorkflow, to: ProtoRpc)
  relation workflow_suggests_order(from: ProtoRpc, to: ProtoRpc)

theory ProtoApiExtensional on ProtoApi:
  -- Extensional constraints inferred from current tuples (best-effort).
  -- Treat these as hypotheses: they may not generalize as new data arrives.

  -- Keys: make fact atoms like `proto_rpc_http_endpoint(from=a, to=b)` eligible for key pruning.
  constraint key proto_rpc_http_endpoint(from, to)
  constraint key proto_rpc_http_endpoint(from)
  constraint functional proto_rpc_http_endpoint.from -> proto_rpc_http_endpoint.to
  constraint key proto_rpc_http_endpoint(to)
  constraint functional proto_rpc_http_endpoint.to -> proto_rpc_http_endpoint.from

  -- Keys: make fact atoms like `proto_rpc_request(from=a, to=b)` eligible for key pruning.
  constraint key proto_rpc_request(from, to)
  constraint key proto_rpc_request(from)
  constraint functional proto_rpc_request.from -> proto_rpc_request.to
  constraint key proto_rpc_request(to)
  constraint functional proto_rpc_request.to -> proto_rpc_request.from

  -- Keys: make fact atoms like `proto_rpc_response(from=a, to=b)` eligible for key pruning.
  constraint key proto_rpc_response(from, to)
  constraint key proto_rpc_response(from)
  constraint functional proto_rpc_response.from -> proto_rpc_response.to
  constraint key proto_rpc_response(to)
  constraint functional proto_rpc_response.to -> proto_rpc_response.from

  -- Keys: make fact atoms like `proto_service_has_rpc(from=a, to=b)` eligible for key pruning.
  constraint key proto_service_has_rpc(from, to)
  constraint key proto_service_has_rpc(to)
  constraint functional proto_service_has_rpc.to -> proto_service_has_rpc.from

  -- Keys: make fact atoms like `proto_service_has_workflow(from=a, to=b)` eligible for key pruning.
  constraint key proto_service_has_workflow(from, to)
  constraint key proto_service_has_workflow(from)
  constraint functional proto_service_has_workflow.from -> proto_service_has_workflow.to
  constraint key proto_service_has_workflow(to)
  constraint functional proto_service_has_workflow.to -> proto_service_has_workflow.from

  -- Keys: make fact atoms like `workflow_includes_rpc(from=a, to=b)` eligible for key pruning.
  constraint key workflow_includes_rpc(from, to)
  constraint key workflow_includes_rpc(to)
  constraint functional workflow_includes_rpc.to -> workflow_includes_rpc.from

  -- Keys: make fact atoms like `workflow_suggests_order(from=a, to=b)` eligible for key pruning.
  constraint key workflow_suggests_order(from, to)
  constraint key workflow_suggests_order(from)
  constraint functional workflow_suggests_order.from -> workflow_suggests_order.to
  constraint key workflow_suggests_order(to)
  constraint functional workflow_suggests_order.to -> workflow_suggests_order.from

instance ProtoApiInstance of ProtoApi:
  ApiWorkflow = {
    UserWorkflow
  }

  HttpEndpoint = {
    GET_v1_users_user_id,
    POST_v1_users
  }

  ProtoMessage = {
    CreateUserRequest,
    CreateUserResponse,
    GetUserRequest,
    GetUserResponse,
    User
  }

  ProtoRpc = {
    CreateUser,
    GetUser
  }

  ProtoService = {
    UserService
  }

  proto_rpc_http_endpoint = {
    (from=CreateUser, to=POST_v1_users),
    (from=GetUser, to=GET_v1_users_user_id)
  }

  proto_rpc_request = {
    (from=CreateUser, to=CreateUserRequest),
    (from=GetUser, to=GetUserRequest)
  }

  proto_rpc_response = {
    (from=CreateUser, to=CreateUserResponse),
    (from=GetUser, to=GetUserResponse)
  }

  proto_service_has_rpc = {
    (from=UserService, to=CreateUser),
    (from=UserService, to=GetUser)
  }

  proto_service_has_workflow = {
    (from=UserService, to=UserWorkflow)
  }

  workflow_includes_rpc = {
    (from=UserWorkflow, to=CreateUser),
    (from=UserWorkflow, to=GetUser)
  }

  workflow_suggests_order = {
    (from=CreateUser, to=GetUser)
  }
