# Knowledge Ingestion Guide

This document describes how to ingest knowledge from various sources into Axiograph.

## Document Sources

### Technical Manuals

Technical manuals contain valuable tacit knowledge about machining processes:

- **Cutting parameters**: recommended speeds, feeds, depths of cut
- **Material properties**: hardness, machinability ratings
- **Tool selection**: which tool for which operation
- **Troubleshooting**: what to do when chatter occurs

### Shop Floor Conversations

Machinists share knowledge verbally:

> "When cutting titanium, keep the speed low but the feed high. 
> The heat needs to go into the chip, not the workpiece."

This can be captured as:

```axiograph
relation CuttingAdvice(mat: Material, parameter: Parameter, direction: Direction)

CuttingAdvice = {
  (mat=Titanium, parameter=Speed, direction=Low),
  (mat=Titanium, parameter=Feed, direction=High)
}
```

### Reference Books

Classic machining references include:

- Machinery's Handbook
- Tool and Manufacturing Engineers Handbook
- ASM Handbook Vol. 16 (Machining)

## Ingestion Pipeline

1. **Extract text** from source documents
2. **Identify entities** (materials, tools, parameters)
3. **Extract relations** (recommendations, observations)
4. **Encode in .axi** format
5. **Validate** against schema constraints
6. **Store** in binary format for fast access

## Example: Tool Wear Observation

From a shop floor log:

> "Noticed significant crater wear on the CNMG insert after 20 minutes 
> cutting Inconel 718 at 150 SFM."

Encoded as:

```axiograph
ObservedWear = {
  (tool=CNMG_Insert, wearType=CraterWear, amount=Significant, ctx=ShopFloor_Log_2024_01_15, time=T20min)
}

CuttingCondition = {
  (obs=Obs_001, mat=Inconel718, speed=SFM150)
}
```

