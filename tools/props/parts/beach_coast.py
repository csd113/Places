"""Shared supported cove profile for authored terrain and foam geometry."""
import math


def coast_z(x):
    """The supported cove profile, shared by terrain and its foam shell."""
    distance = abs(x+1)
    tangent = 25/6
    bend = (.01*distance*distance if distance <= tangent
            else .01*tangent*tangent + (distance-tangent)/12)
    return round((-.9 + bend)*64)/64


def cove_foam_profile():
    """Continuous scalloped edges in authored world X/Z, with sealed ends."""
    rows = []
    for index in range(65):
        x = -24 + index*.75
        center = coast_z(x)+.78+.035*math.sin(x*.72)
        half = .15+.028*math.sin(x*1.13)+.017*math.sin(x*2.1)
        rows.append(((x, center-half), (x, center+half)))
    return rows


def cove_foam_bounds():
    rows = cove_foam_profile()
    zs = [point[1] for row in rows for point in row]
    return min(zs), max(zs)
