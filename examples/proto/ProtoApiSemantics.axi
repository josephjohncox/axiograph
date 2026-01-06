-- Proto API semantics (theory + first-class axioms)
--
-- This is a canonical `.axi` module intended to be:
-- - small and readable,
-- - easy to export back out of PathDB via `export_axi_module`,
-- - and useful as an example of **theories / axioms** applied to a proto/gRPC-ish
--   API surface.
--
-- It demonstrates:
-- - a schema for services/RPCs/endpoints + doc chunks,
-- - a theory block with extensional constraints (keys/functionals), and
-- - first-class rewrite rules (certificate-addressable semantics).

module ProtoApiSemantics

schema ProtoApiSemantics:
  -- A safe fallback supertype.
  object Entity

  object ProtoService
  object ProtoRpc
  object HttpEndpoint
  object DocChunk

  -- Optional: some examples (and test harnesses) use `Homotopy` as a generic
  -- "witness node" type. We include one here so default REPL smoke queries can
  -- always return something.
  object Homotopy

  subtype ProtoService < Entity
  subtype ProtoRpc < Entity
  subtype HttpEndpoint < Entity
  subtype DocChunk < Entity
  subtype Homotopy < Entity

  -- Core structural relations.
  relation proto_service_has_rpc(service: ProtoService, rpc: ProtoRpc)
  relation proto_rpc_http_endpoint(rpc: ProtoRpc, endpoint: HttpEndpoint)

  -- A derived, meaning-level relation we want to treat as part of the ontology.
  relation proto_service_has_endpoint(service: ProtoService, endpoint: HttpEndpoint)

  -- Optional “reverse edge” (useful for traversal + rewrite rules).
  relation proto_http_endpoint_of_rpc(endpoint: HttpEndpoint, rpc: ProtoRpc)

  -- Evidence-plane-ish: a doc chunk can mention an RPC or an endpoint.
  relation mentions_rpc(doc: DocChunk, rpc: ProtoRpc)
  relation mentions_http_endpoint(doc: DocChunk, endpoint: HttpEndpoint)

theory ProtoApiRules on ProtoApiSemantics:
  -- --------------------------------------------------------------------------
  -- Extensional constraints (hygiene)
  -- --------------------------------------------------------------------------

  constraint key proto_service_has_rpc(service, rpc)

  constraint key proto_rpc_http_endpoint(rpc, endpoint)
  constraint key proto_rpc_http_endpoint(rpc)
  constraint functional proto_rpc_http_endpoint.rpc -> proto_rpc_http_endpoint.endpoint

  constraint key proto_http_endpoint_of_rpc(endpoint, rpc)
  constraint key proto_http_endpoint_of_rpc(endpoint)
  constraint functional proto_http_endpoint_of_rpc.endpoint -> proto_http_endpoint_of_rpc.rpc

  constraint key proto_service_has_endpoint(service, endpoint)

  constraint key mentions_rpc(doc, rpc)
  constraint key mentions_http_endpoint(doc, endpoint)

  -- --------------------------------------------------------------------------
  -- Axioms / rewrite rules (first-class, certificate-addressable)
  -- --------------------------------------------------------------------------

  -- Definitional/compositional semantics:
  -- "A service has an endpoint" means "it has an RPC that maps to that endpoint".
  rewrite service_has_endpoint_def:
    orientation: bidirectional
    vars: svc: ProtoService, rpc: ProtoRpc, ep: HttpEndpoint
    lhs: trans(step(svc, proto_service_has_rpc, rpc), step(rpc, proto_rpc_http_endpoint, ep))
    rhs: step(svc, proto_service_has_endpoint, ep)

  -- Declare `proto_http_endpoint_of_rpc` as the inverse of `proto_rpc_http_endpoint`.
  rewrite endpoint_of_rpc_is_inverse:
    orientation: bidirectional
    vars: rpc: ProtoRpc, ep: HttpEndpoint
    lhs: step(rpc, proto_rpc_http_endpoint, ep)
    rhs: inv(step(ep, proto_http_endpoint_of_rpc, rpc))

  -- A doc can reach an RPC either by mentioning it directly, or by mentioning an
  -- endpoint and following the endpoint→rpc edge.
  --
  -- This rule is intentionally *directed*: we allow simplifying the longer path
  -- into `mentions_rpc`, but not the other direction.
  rewrite doc_mentions_rpc_via_endpoint:
    orientation: forward
    vars: d: DocChunk, ep: HttpEndpoint, rpc: ProtoRpc
    lhs: trans(step(d, mentions_http_endpoint, ep), step(ep, proto_http_endpoint_of_rpc, rpc))
    rhs: step(d, mentions_rpc, rpc)

instance ProtoApiTiny of ProtoApiSemantics:
  ProtoService = {UserService}
  ProtoRpc = {GetUser, CreateUser}
  HttpEndpoint = {GET_v1_users_user_id, POST_v1_users}
  DocChunk = {Doc_UserService_Overview}
  Homotopy = {homotopy_doc_mentions_getuser_0}

  proto_service_has_rpc = {
    (service=UserService, rpc=GetUser),
    (service=UserService, rpc=CreateUser)
  }

  proto_rpc_http_endpoint = {
    (rpc=GetUser, endpoint=GET_v1_users_user_id),
    (rpc=CreateUser, endpoint=POST_v1_users)
  }

  proto_http_endpoint_of_rpc = {
    (endpoint=GET_v1_users_user_id, rpc=GetUser),
    (endpoint=POST_v1_users, rpc=CreateUser)
  }

  mentions_rpc = {
    (doc=Doc_UserService_Overview, rpc=GetUser)
  }

  mentions_http_endpoint = {
    (doc=Doc_UserService_Overview, endpoint=GET_v1_users_user_id)
  }
