#!/usr/bin/env python3
import argparse
from pathlib import Path


def build_model(out_path: Path, scale: float, bias: float) -> None:
    try:
        import onnx
        from onnx import TensorProto, helper
    except Exception as exc:  # pragma: no cover
        raise SystemExit(
            "onnx is required to build the ONNX world model. Install with: pip install onnx"
        ) from exc

    input_info = helper.make_tensor_value_info("seed", TensorProto.INT64, [1])
    output_info = helper.make_tensor_value_info("score", TensorProto.FLOAT, [1])

    scale_tensor = helper.make_tensor("scale", TensorProto.FLOAT, [1], [scale])
    bias_tensor = helper.make_tensor("bias", TensorProto.FLOAT, [1], [bias])

    nodes = [
        helper.make_node("Cast", ["seed"], ["seed_f"], to=TensorProto.FLOAT),
        helper.make_node("Mul", ["seed_f", "scale"], ["scaled"]),
        helper.make_node("Add", ["scaled", "bias"], ["logits"]),
        helper.make_node("Sigmoid", ["logits"], ["score"]),
    ]

    graph = helper.make_graph(
        nodes,
        "axiograph_world_model_small",
        [input_info],
        [output_info],
        initializer=[scale_tensor, bias_tensor],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)])

    out_path.parent.mkdir(parents=True, exist_ok=True)
    onnx.save(model, str(out_path))


def main() -> None:
    parser = argparse.ArgumentParser(description="Build a minimal ONNX world model.")
    parser.add_argument(
        "--out",
        default="models/world_model_small.onnx",
        help="Output ONNX model path",
    )
    parser.add_argument(
        "--scale",
        type=float,
        default=1e-6,
        help="Scale applied before sigmoid (default: 1e-6)",
    )
    parser.add_argument(
        "--bias",
        type=float,
        default=0.0,
        help="Bias applied before sigmoid (default: 0.0)",
    )
    args = parser.parse_args()

    build_model(Path(args.out), args.scale, args.bias)
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
