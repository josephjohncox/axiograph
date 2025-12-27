-- Candidate `.axi` module produced from `proposals.json` (schema discovery).
--
-- This file is intentionally small and reviewable. It demonstrates:
-- - drafting a schema from proto/gRPC-ish proposals,
-- - capturing both structural knowledge (services/rpcs/messages/endpoints),
-- - and a tiny amount of tacit knowledge (workflow grouping + suggested ordering).
--
-- Re-generate (from repo root):
--   cd rust
--   cargo run -p axiograph-cli -- discover draft-module \
--     ../examples/schema_discovery/proto_api_proposals.json \
--     --out ../build/ProtoApi.proposals.axi \
--     --module ProtoApi_Proposals \
--     --schema ProtoApi \
--     --instance ProtoApiInstance \
--     --infer-constraints

module ProtoApi_Proposals

schema ProtoApi:
  -- Safe fallback supertype for heterogeneous endpoints.
  object Entity

  -- Types observed in proposals.
  object ProtoService
  object ProtoRpc
  object ProtoMessage
  object HttpEndpoint
  object ApiWorkflow

  subtype ProtoService < Entity
  subtype ProtoRpc < Entity
  subtype ProtoMessage < Entity
  subtype HttpEndpoint < Entity
  subtype ApiWorkflow < Entity

  -- Relations observed in proposals.
  relation proto_service_has_rpc(from: ProtoService, to: ProtoRpc)
  relation proto_rpc_http_endpoint(from: ProtoRpc, to: HttpEndpoint)
  relation proto_rpc_request(from: ProtoRpc, to: ProtoMessage)
  relation proto_rpc_response(from: ProtoRpc, to: ProtoMessage)

  -- Tacit / heuristic structures.
  relation proto_service_has_workflow(from: ProtoService, to: ApiWorkflow)
  relation workflow_includes_rpc(from: ApiWorkflow, to: ProtoRpc)
  relation workflow_suggests_order(from: ProtoRpc, to: ProtoRpc)

theory ProtoApiExtensional on ProtoApi:
  -- Extensional constraints inferred from the observed tuples (hypotheses).
  --
  -- These are not "true about the world"; they are a best-effort summary of
  -- the current evidence and can be invalidated by new data later.

  constraint key proto_service_has_rpc(from, to)

  constraint key proto_rpc_http_endpoint(from, to)
  constraint key proto_rpc_http_endpoint(from)
  constraint functional proto_rpc_http_endpoint.from -> proto_rpc_http_endpoint.to

  constraint key proto_rpc_request(from, to)
  constraint key proto_rpc_request(from)
  constraint functional proto_rpc_request.from -> proto_rpc_request.to

  constraint key proto_rpc_response(from, to)
  constraint key proto_rpc_response(from)
  constraint functional proto_rpc_response.from -> proto_rpc_response.to

  constraint key proto_service_has_workflow(from, to)
  constraint key proto_service_has_workflow(from)
  constraint functional proto_service_has_workflow.from -> proto_service_has_workflow.to

  constraint key workflow_includes_rpc(from, to)

  constraint key workflow_suggests_order(from, to)
  constraint key workflow_suggests_order(from)
  constraint functional workflow_suggests_order.from -> workflow_suggests_order.to

instance ProtoApiInstance of ProtoApi:
  ProtoService = {UserService}

  ProtoRpc = {GetUser, CreateUser}

  ProtoMessage = {
    User,
    GetUserRequest,
    GetUserResponse,
    CreateUserRequest,
    CreateUserResponse
  }

  HttpEndpoint = {
    GET_v1_users_user_id,
    POST_v1_users
  }

  ApiWorkflow = {UserWorkflow}

  proto_service_has_rpc = {
    (from=UserService, to=GetUser),
    (from=UserService, to=CreateUser)
  }

  proto_rpc_http_endpoint = {
    (from=GetUser, to=GET_v1_users_user_id),
    (from=CreateUser, to=POST_v1_users)
  }

  proto_rpc_request = {
    (from=GetUser, to=GetUserRequest),
    (from=CreateUser, to=CreateUserRequest)
  }

  proto_rpc_response = {
    (from=GetUser, to=GetUserResponse),
    (from=CreateUser, to=CreateUserResponse)
  }

  proto_service_has_workflow = {
    (from=UserService, to=UserWorkflow)
  }

  workflow_includes_rpc = {
    (from=UserWorkflow, to=GetUser),
    (from=UserWorkflow, to=CreateUser)
  }

  workflow_suggests_order = {
    (from=CreateUser, to=GetUser)
  }

